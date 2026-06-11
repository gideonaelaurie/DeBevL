use bevy::prelude::*;
use bevy::app::AppExit;
use std::path::PathBuf;
use std::time::SystemTime;
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use serde::{Deserialize, Serialize};

use crate::{
    AppFonts, DialogChannel, RunningProcesses, StoreGames, StoreChannel,
    ActiveDownloads, DownloadChannel, ProjectStatus, DownloadResult,
    get_game_download_path,
};
use crate::config::{save_config, LauncherConfig, Project};
use crate::launcher::{detect_project_type, launch_project, ProjectType};

#[derive(Event)]
pub struct UpdateUiEvent;

#[derive(Component)]
pub struct ProjectListContainer;

#[derive(Component)]
pub struct ProjectCard;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Resource, Serialize, Deserialize)]
pub enum ActiveTab {
    #[default]
    MyGames,
    Store,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ButtonType {
    SelectFolder,
    Launch,
    Stop,
    Delete,
    Tab(ActiveTab),
    OpenLink,
    Quit,
    Download,
}

#[derive(Component, Clone)]
pub enum ButtonAction {
    SelectFolder,
    Launch(PathBuf),
    Stop(PathBuf),
    Delete(PathBuf),
    SwitchTab(ActiveTab),
    OpenUrl(String),
    Quit,
    Download { name: String, url: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, Resource)]
pub struct UiConfig {
    pub active_tab: ActiveTab,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            active_tab: ActiveTab::MyGames,
        }
    }
}

pub fn get_ui_config_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let mut path = PathBuf::from(home);
    path.push(".debevl");
    path.push("28bevUI");
    Some(path)
}

pub fn load_ui_config() -> UiConfig {
    let Some(mut path) = get_ui_config_path() else {
        return UiConfig::default();
    };
    path.push("ui.json");

    if !path.exists() {
        return UiConfig::default();
    }

    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => return UiConfig::default(),
    };

    let mut contents = String::new();
    if file.read_to_string(&mut contents).is_err() {
        return UiConfig::default();
    }

    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_ui_config(config: &UiConfig) {
    let Some(dir_path) = get_ui_config_path() else {
        return;
    };

    if !dir_path.exists() && create_dir_all(&dir_path).is_err() {
        return;
    }

    let mut path = dir_path.clone();
    path.push("ui.json");

    let Ok(serialized) = serde_json::to_string_pretty(config) else {
        return;
    };

    if let Ok(mut file) = File::create(&path) {
        let _ = file.write_all(serialized.as_bytes());
    }
}

#[allow(non_camel_case_types)]
pub struct _28bevUI;

impl Plugin for _28bevUI {
    fn build(&self, app: &mut App) {
        let ui_config = load_ui_config();

        app.insert_resource(ui_config.active_tab)
            .add_event::<UpdateUiEvent>()
            .add_systems(Update, (
                process_dialog_returns,
                process_store_returns,
                process_download_returns,
                handle_ui_buttons,
                button_system,
                card_hover_system,
                update_tab_buttons,
                update_ui_list,
            ));
    }
}

pub fn setup_ui(mut commands: Commands, fonts: Res<AppFonts>) {
    // 2D Camera for UI
    commands.spawn(Camera2d);

    // Root UI container
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::all(Val::Px(20.0)),
            ..default()
        },
    )).with_children(|parent| {
        // Main Glassmorphic Panel
        parent.spawn((
            Node {
                width: Val::Px(720.0),
                height: Val::Percent(92.0),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::all(Val::Px(24.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(16.0)),
            BackgroundColor(Color::srgba(0.04, 0.04, 0.06, 0.88)),
            BorderColor(Color::srgba(0.2, 0.25, 0.35, 0.35)),
        )).with_children(|panel| {
            // Header Row
            panel.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                margin: UiRect::bottom(Val::Px(16.0)),
                ..default()
            }).with_children(|header| {
                // Title and Subtitle Block
                header.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                }).with_children(|title_block| {
                    title_block.spawn((
                        Text::new("DeBevL"),
                        TextFont {
                            font: fonts.semibold.clone(),
                            font_size: 28.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.0, 0.8, 1.0)),
                    ));
                    title_block.spawn((
                        Text::new("Dedicated Bevy Game Launcher"),
                        TextFont {
                            font: fonts.regular.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.6, 0.6, 0.7, 0.7)),
                    ));
                });

                // Controls Block (Select Folder & Quit)
                header.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    align_items: AlignItems::Center,
                    ..default()
                }).with_children(|controls| {
                    // "Select Folder" Button
                    controls.spawn((
                        Button,
                        Node {
                            padding: UiRect::new(Val::Px(16.0), Val::Px(16.0), Val::Px(10.0), Val::Px(10.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderRadius::all(Val::Px(8.0)),
                        BackgroundColor(Color::srgb(0.08, 0.45, 0.9)),
                        ButtonType::SelectFolder,
                        ButtonAction::SelectFolder,
                    )).with_child((
                        Text::new("Select Game Folder..."),
                        TextFont {
                            font: fonts.semibold.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    // "Quit" Button
                    controls.spawn((
                        Button,
                        Node {
                            padding: UiRect::new(Val::Px(16.0), Val::Px(16.0), Val::Px(10.0), Val::Px(10.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderRadius::all(Val::Px(8.0)),
                        BackgroundColor(Color::srgb(0.7, 0.1, 0.1)),
                        ButtonType::Quit,
                        ButtonAction::Quit,
                    )).with_child((
                        Text::new("Quit"),
                        TextFont {
                            font: fonts.semibold.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            });

            // Tab Navigation Bar
            panel.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                margin: UiRect::bottom(Val::Px(12.0)),
                ..default()
            }).with_children(|tabs| {
                // "My Games" Tab Button
                tabs.spawn((
                    Button,
                    Node {
                        padding: UiRect::new(Val::Px(16.0), Val::Px(16.0), Val::Px(8.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderRadius::all(Val::Px(6.0)),
                    BackgroundColor(Color::srgb(0.08, 0.45, 0.9)), // Active by default
                    BorderColor(Color::srgb(0.0, 0.8, 1.0)),
                    ButtonType::Tab(ActiveTab::MyGames),
                    ButtonAction::SwitchTab(ActiveTab::MyGames),
                )).with_child((
                    Text::new("My Games"),
                    TextFont {
                        font: fonts.semibold.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));

                // "GitHub Store" Tab Button
                tabs.spawn((
                    Button,
                    Node {
                        padding: UiRect::new(Val::Px(16.0), Val::Px(16.0), Val::Px(8.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderRadius::all(Val::Px(6.0)),
                    BackgroundColor(Color::srgba(0.12, 0.12, 0.15, 0.6)), // Inactive by default
                    BorderColor(Color::srgba(0.2, 0.25, 0.35, 0.35)),
                    ButtonType::Tab(ActiveTab::Store),
                    ButtonAction::SwitchTab(ActiveTab::Store),
                )).with_child((
                    Text::new("GitHub Store"),
                    TextFont {
                        font: fonts.semibold.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });

            // Divider Line
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(1.0),
                    margin: UiRect::bottom(Val::Px(16.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.2, 0.25, 0.35, 0.3)),
            ));

            // Project List Scroll/Column Container
            panel.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(70.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                ProjectListContainer,
            ));

            // Footer
            panel.spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                margin: UiRect::top(Val::Px(16.0)),
                ..default()
            }).with_children(|footer| {
                footer.spawn((
                    Text::new("Tip: Run from terminal using 'debevl <folder>' to launch immediately."),
                    TextFont {
                        font: fonts.regular.clone(),
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.5, 0.5, 0.6, 0.6)),
                ));
                footer.spawn((
                    Text::new("v1.0.0"),
                    TextFont {
                        font: fonts.regular.clone(),
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.5, 0.5, 0.6, 0.4)),
                ));
            });
        });
    });
}

pub fn button_system(
    active_tab: Res<ActiveTab>,
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor, &ButtonType),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut bg_color, mut border_color, button_type) in &mut interaction_query {
        match button_type {
            ButtonType::SelectFolder => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgb(0.06, 0.35, 0.75).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgb(0.12, 0.55, 1.0).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgb(0.08, 0.45, 0.9).into();
                }
            },
            ButtonType::Launch => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgb(0.08, 0.5, 0.15).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgb(0.15, 0.7, 0.25).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgb(0.1, 0.6, 0.2).into();
                }
            },
            ButtonType::Stop => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgb(0.55, 0.08, 0.08).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgb(0.8, 0.15, 0.15).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgb(0.7, 0.1, 0.1).into();
                }
            },
            ButtonType::Delete => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgba(0.12, 0.12, 0.15, 0.9).into();
                    *border_color = Color::srgba(0.5, 0.5, 0.5, 0.8).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgba(0.25, 0.25, 0.3, 0.9).into();
                    *border_color = Color::srgba(0.8, 0.4, 0.4, 0.8).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgba(0.2, 0.2, 0.22, 0.8).into();
                    *border_color = Color::srgba(0.3, 0.3, 0.3, 0.5).into();
                }
            },
            ButtonType::Tab(tab) => {
                let is_active = *tab == *active_tab;
                match interaction {
                    Interaction::Pressed => {
                        *bg_color = if is_active { Color::srgb(0.06, 0.35, 0.75).into() } else { Color::srgba(0.2, 0.2, 0.25, 0.8).into() };
                    }
                    Interaction::Hovered => {
                        *bg_color = if is_active { Color::srgb(0.12, 0.55, 1.0).into() } else { Color::srgba(0.25, 0.25, 0.3, 0.8).into() };
                        *border_color = Color::srgba(0.0, 0.8, 1.0, 0.6).into();
                    }
                    Interaction::None => {
                        *bg_color = if is_active { Color::srgb(0.08, 0.45, 0.9).into() } else { Color::srgba(0.12, 0.12, 0.15, 0.6).into() };
                        *border_color = if is_active { Color::srgb(0.0, 0.8, 1.0).into() } else { Color::srgba(0.2, 0.25, 0.35, 0.35).into() };
                    }
                }
            }
            ButtonType::OpenLink => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgb(0.06, 0.35, 0.75).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgb(0.12, 0.55, 1.0).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgb(0.08, 0.45, 0.9).into();
                }
            },
            ButtonType::Quit => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgb(0.55, 0.08, 0.08).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgb(0.8, 0.15, 0.15).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgb(0.7, 0.1, 0.1).into();
                }
            },
            ButtonType::Download => match interaction {
                Interaction::Pressed => {
                    *bg_color = Color::srgb(0.08, 0.5, 0.15).into();
                }
                Interaction::Hovered => {
                    *bg_color = Color::srgb(0.15, 0.7, 0.25).into();
                }
                Interaction::None => {
                    *bg_color = Color::srgb(0.1, 0.6, 0.2).into();
                }
            },
        }
    }
}

pub fn card_hover_system(
    mut query: Query<(&Interaction, &mut BorderColor), (With<ProjectCard>, Changed<Interaction>)>,
) {
    for (interaction, mut border_color) in &mut query {
        match interaction {
            Interaction::Pressed => {
                *border_color = Color::srgb(0.0, 0.8, 1.0).into();
            }
            Interaction::Hovered => {
                *border_color = Color::srgba(0.0, 0.8, 1.0, 0.6).into();
            }
            Interaction::None => {
                *border_color = Color::srgba(0.2, 0.25, 0.35, 0.35).into();
            }
        }
    }
}

pub fn update_tab_buttons(
    active_tab: Res<ActiveTab>,
    mut query: Query<(&mut BackgroundColor, &mut BorderColor, &ButtonAction)>,
) {
    if active_tab.is_changed() {
        for (mut bg_color, mut border_color, action) in &mut query {
            if let ButtonAction::SwitchTab(tab) = action {
                if *tab == *active_tab {
                    *bg_color = Color::srgb(0.08, 0.45, 0.9).into();
                    *border_color = Color::srgb(0.0, 0.8, 1.0).into();
                } else {
                    *bg_color = Color::srgba(0.12, 0.12, 0.15, 0.6).into();
                    *border_color = Color::srgba(0.2, 0.25, 0.35, 0.35).into();
                }
            }
        }
    }
}

pub fn update_ui_list(
    mut commands: Commands,
    fonts: Res<AppFonts>,
    config: Res<LauncherConfig>,
    running: Res<RunningProcesses>,
    active_tab: Res<ActiveTab>,
    store_games: Res<StoreGames>,
    active_downloads: Res<ActiveDownloads>,
    container_query: Query<Entity, With<ProjectListContainer>>,
    mut event_reader: EventReader<UpdateUiEvent>,
) {
    let mut should_update = false;
    for _ in event_reader.read() {
        should_update = true;
    }

    if !should_update {
        return;
    }

    let Ok(container_entity) = container_query.get_single() else {
        return;
    };

    commands.entity(container_entity).despawn_descendants();

    let statuses = running.statuses.lock().unwrap();

    commands.entity(container_entity).with_children(|parent| {
        match *active_tab {
            ActiveTab::MyGames => {
                if config.projects.is_empty() {
                    parent.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(160.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        row_gap: Val::Px(8.0),
                        ..default()
                    }).with_children(|ph| {
                        ph.spawn((
                            Text::new("No Bevy games added to history yet."),
                            TextFont {
                                font: fonts.regular.clone(),
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.6, 0.6, 0.7, 0.7)),
                        ));
                        ph.spawn((
                            Text::new("Click 'Select Game Folder' to choose a Bevy folder to run."),
                            TextFont {
                                font: fonts.regular.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.5, 0.5, 0.6, 0.5)),
                        ));
                    });
                    return;
                }

                // Show the 5 most recently used projects
                let mut sorted_projects = config.projects.clone();
                sorted_projects.sort_by(|a, b| b.last_launched.unwrap_or(0).cmp(&a.last_launched.unwrap_or(0)));

                for project in sorted_projects.iter().take(5) {
                    let status = statuses.get(&project.path).copied().unwrap_or(ProjectStatus::Idle);
                    let path_str = project.path.to_string_lossy().into_owned();

                    parent.spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(14.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderRadius::all(Val::Px(10.0)),
                        BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.5)),
                        BorderColor(Color::srgba(0.2, 0.25, 0.35, 0.35)),
                        ProjectCard,
                    )).with_children(|card| {
                        // Info block
                        card.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            ..default()
                        }).with_children(|info| {
                            info.spawn((
                                Text::new(project.name.clone()),
                                TextFont {
                                    font: fonts.semibold.clone(),
                                    font_size: 16.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                            info.spawn((
                                Text::new(path_str.clone()),
                                TextFont {
                                    font: fonts.regular.clone(),
                                    font_size: 11.0,
                                    ..default()
                                },
                                TextColor(Color::srgba(0.5, 0.5, 0.6, 0.7)),
                            ));
                        });

                        // Status & Actions block
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(12.0),
                            ..default()
                        }).with_children(|actions| {
                            // Badge details
                            let (badge_text, badge_color, badge_border_color, badge_text_color) = match status {
                                ProjectStatus::Idle => (
                                    "Idle",
                                    Color::srgba(0.2, 0.2, 0.25, 0.2),
                                    Color::srgba(0.3, 0.35, 0.45, 0.4),
                                    Color::srgba(0.65, 0.65, 0.75, 0.75),
                                ),
                                ProjectStatus::Running => (
                                    "Running",
                                    Color::srgba(0.08, 0.6, 0.2, 0.12),
                                    Color::srgba(0.12, 0.65, 0.25, 0.4),
                                    Color::srgb(0.2, 0.85, 0.4),
                                ),
                                ProjectStatus::FailedLaunch => (
                                    "Failed",
                                    Color::srgba(0.75, 0.08, 0.08, 0.12),
                                    Color::srgba(0.8, 0.12, 0.12, 0.4),
                                    Color::srgb(0.95, 0.3, 0.3),
                                ),
                                ProjectStatus::Invalid => (
                                    "Invalid",
                                    Color::srgba(0.8, 0.4, 0.0, 0.12),
                                    Color::srgba(0.8, 0.45, 0.0, 0.4),
                                    Color::srgb(1.0, 0.6, 0.1),
                                ),
                            };

                            // Badge node
                            actions.spawn((
                                Node {
                                    padding: UiRect::new(Val::Px(8.0), Val::Px(8.0), Val::Px(3.0), Val::Px(3.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BorderRadius::all(Val::Px(4.0)),
                                BackgroundColor(badge_color),
                                BorderColor(badge_border_color),
                            )).with_child((
                                Text::new(badge_text),
                                TextFont {
                                    font: fonts.regular.clone(),
                                    font_size: 11.0,
                                    ..default()
                                },
                                TextColor(badge_text_color),
                            ));

                            // Action buttons
                            if status == ProjectStatus::Running {
                                // Stop button
                                actions.spawn((
                                    Button,
                                    Node {
                                        padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderRadius::all(Val::Px(6.0)),
                                    BackgroundColor(Color::srgb(0.7, 0.1, 0.1)),
                                    ButtonType::Stop,
                                    ButtonAction::Stop(project.path.clone()),
                                )).with_child((
                                    Text::new("Stop"),
                                    TextFont {
                                        font: fonts.semibold.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                            } else {
                                // Launch button
                                actions.spawn((
                                    Button,
                                    Node {
                                        padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderRadius::all(Val::Px(6.0)),
                                    BackgroundColor(Color::srgb(0.1, 0.6, 0.2)),
                                    ButtonType::Launch,
                                    ButtonAction::Launch(project.path.clone()),
                                )).with_child((
                                    Text::new("Launch"),
                                    TextFont {
                                        font: fonts.semibold.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                            }

                            // Delete button
                            actions.spawn((
                                Button,
                                Node {
                                    padding: UiRect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(6.0), Val::Px(6.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BorderRadius::all(Val::Px(6.0)),
                                BackgroundColor(Color::srgba(0.2, 0.2, 0.22, 0.8)),
                                BorderColor(Color::srgba(0.3, 0.3, 0.3, 0.5)),
                                ButtonType::Delete,
                                ButtonAction::Delete(project.path.clone()),
                            )).with_child((
                                Text::new("X"),
                                TextFont {
                                    font: fonts.semibold.clone(),
                                    font_size: 12.0,
                                    ..default()
                                },
                                TextColor(Color::srgba(0.7, 0.7, 0.7, 0.8)),
                            ));
                        });
                    });
                }
            }
            ActiveTab::Store => {
                if store_games.loading {
                    parent.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(160.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    }).with_child((
                        Text::new("Searching GitHub for Bevy games..."),
                        TextFont {
                            font: fonts.semibold.clone(),
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.6, 0.7, 0.9, 0.8)),
                    ));
                    return;
                }

                if let Some(ref err) = store_games.error {
                    parent.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(160.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        row_gap: Val::Px(8.0),
                        ..default()
                    }).with_children(|ph| {
                        ph.spawn((
                            Text::new("Failed to connect to GitHub Store."),
                            TextFont {
                                font: fonts.semibold.clone(),
                                font_size: 15.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.9, 0.3, 0.3)),
                        ));
                        ph.spawn((
                            Text::new(err.clone()),
                            TextFont {
                                font: fonts.regular.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.7, 0.5, 0.5, 0.7)),
                        ));
                    });
                    return;
                }

                if store_games.items.is_empty() {
                    parent.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(160.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    }).with_child((
                        Text::new("No Bevy games found on GitHub."),
                        TextFont {
                            font: fonts.regular.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.6, 0.6, 0.7, 0.7)),
                    ));
                    return;
                }

                // Show top 5 repositories from GitHub
                for item in store_games.items.iter().take(5) {
                    parent.spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(14.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderRadius::all(Val::Px(10.0)),
                        BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.5)),
                        BorderColor(Color::srgba(0.2, 0.25, 0.35, 0.35)),
                        ProjectCard,
                    )).with_children(|card| {
                        // Info Block
                        card.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            width: Val::Percent(65.0),
                            ..default()
                        }).with_children(|info| {
                            info.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(8.0),
                                align_items: AlignItems::Center,
                                ..default()
                            }).with_children(|title_row| {
                                title_row.spawn((
                                    Text::new(item.name.clone()),
                                    TextFont {
                                        font: fonts.semibold.clone(),
                                        font_size: 16.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                                title_row.spawn((
                                    Text::new(format!("by {}", item.owner)),
                                    TextFont {
                                        font: fonts.regular.clone(),
                                        font_size: 11.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgba(0.5, 0.5, 0.6, 0.8)),
                                ));
                            });

                            info.spawn((
                                Text::new(item.description.clone()),
                                TextFont {
                                    font: fonts.regular.clone(),
                                    font_size: 11.0,
                                    ..default()
                                },
                                TextColor(Color::srgba(0.5, 0.5, 0.6, 0.7)),
                            ));
                        });

                        // Actions Block
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(12.0),
                            ..default()
                        }).with_children(|actions| {
                            // Stars Badge
                            actions.spawn((
                                Node {
                                    padding: UiRect::new(Val::Px(8.0), Val::Px(8.0), Val::Px(3.0), Val::Px(3.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BorderRadius::all(Val::Px(4.0)),
                                BackgroundColor(Color::srgba(0.9, 0.7, 0.1, 0.12)),
                                BorderColor(Color::srgba(0.9, 0.7, 0.1, 0.4)),
                            )).with_child((
                                Text::new(format!("★ {}", item.stars)),
                                TextFont {
                                    font: fonts.semibold.clone(),
                                    font_size: 11.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.75, 0.1)),
                            ));

                            // Open Link Button
                            actions.spawn((
                                Button,
                                Node {
                                    padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BorderRadius::all(Val::Px(6.0)),
                                BackgroundColor(Color::srgb(0.08, 0.45, 0.9)),
                                ButtonType::OpenLink,
                                ButtonAction::OpenUrl(item.url.clone()),
                            )).with_child((
                                Text::new("Open GitHub"),
                                TextFont {
                                    font: fonts.semibold.clone(),
                                    font_size: 12.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));

                            // Download/Status Button
                            let download_path = get_game_download_path(&item.name);
                            let is_downloaded = download_path.as_ref().map(|p| p.exists()).unwrap_or(false);
                            let is_downloading = active_downloads.names.contains(&item.name);

                            if is_downloaded {
                                actions.spawn((
                                    Node {
                                        padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderRadius::all(Val::Px(6.0)),
                                    BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.4)),
                                )).with_child((
                                    Text::new("In Library"),
                                    TextFont {
                                        font: fonts.semibold.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgba(0.6, 0.6, 0.7, 0.5)),
                                ));
                            } else if is_downloading {
                                actions.spawn((
                                    Node {
                                        padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderRadius::all(Val::Px(6.0)),
                                    BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.4)),
                                )).with_child((
                                    Text::new("Downloading..."),
                                    TextFont {
                                        font: fonts.semibold.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgba(0.6, 0.6, 0.7, 0.5)),
                                ));
                            } else {
                                actions.spawn((
                                    Button,
                                    Node {
                                        padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderRadius::all(Val::Px(6.0)),
                                    BackgroundColor(Color::srgb(0.1, 0.6, 0.2)),
                                    ButtonType::Download,
                                    ButtonAction::Download {
                                        name: item.name.clone(),
                                        url: item.url.clone(),
                                    },
                                )).with_child((
                                    Text::new("Download"),
                                    TextFont {
                                        font: fonts.semibold.clone(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                            }
                        });
                    });
                }
            }
        }
    });
}

pub fn handle_ui_buttons(
    interaction_query: Query<(&Interaction, &ButtonAction), (Changed<Interaction>, With<Button>)>,
    mut config: ResMut<LauncherConfig>,
    running: Res<RunningProcesses>,
    dialog_channel: Res<DialogChannel>,
    download_channel: Res<DownloadChannel>,
    mut active_downloads: ResMut<ActiveDownloads>,
    mut ui_events: EventWriter<UpdateUiEvent>,
    mut active_tab: ResMut<ActiveTab>,
    mut app_exit: EventWriter<AppExit>,
) {
    for (interaction, action) in &interaction_query {
        if *interaction == Interaction::Pressed {
            match action {
                ButtonAction::SelectFolder => {
                    let tx = dialog_channel.tx.clone();
                    std::thread::spawn(move || {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            let _ = tx.send(path);
                        }
                    });
                }
                ButtonAction::Launch(path) => {
                    let mut map = running.map.lock().unwrap();
                    let mut statuses = running.statuses.lock().unwrap();

                    match launch_project(path) {
                        Ok(child) => {
                            map.insert(path.clone(), child);
                            statuses.insert(path.clone(), ProjectStatus::Running);

                            // Update last launched
                            if let Some(proj) = config.projects.iter_mut().find(|p| p.path == *path) {
                                let now = SystemTime::now()
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();
                                proj.last_launched = Some(now);
                            }
                            save_config(&config);
                            ui_events.send(UpdateUiEvent);
                        }
                        Err(e) => {
                            println!("Error launching {}: {}", path.display(), e);
                            statuses.insert(path.clone(), ProjectStatus::FailedLaunch);
                            ui_events.send(UpdateUiEvent);
                        }
                    }
                }
                ButtonAction::Stop(path) => {
                    let mut map = running.map.lock().unwrap();
                    let mut statuses = running.statuses.lock().unwrap();

                    if let Some(mut child) = map.remove(path) {
                        let _ = child.kill();
                    }
                    statuses.insert(path.clone(), ProjectStatus::Idle);
                    ui_events.send(UpdateUiEvent);
                }
                ButtonAction::Delete(path) => {
                    let mut map = running.map.lock().unwrap();
                    let mut statuses = running.statuses.lock().unwrap();

                    if let Some(mut child) = map.remove(path) {
                        let _ = child.kill();
                    }
                    statuses.remove(path);

                    config.projects.retain(|p| p.path != *path);
                    save_config(&config);
                    ui_events.send(UpdateUiEvent);
                }
                ButtonAction::SwitchTab(tab) => {
                    *active_tab = *tab;
                    ui_events.send(UpdateUiEvent);

                    // Save UI config to state file in .debevl/28ui/ui.json
                    let ui_config = UiConfig {
                        active_tab: *tab,
                    };
                    save_ui_config(&ui_config);
                }
                ButtonAction::OpenUrl(url) => {
                    let _ = webbrowser::open(url);
                }
                ButtonAction::Quit => {
                    app_exit.send(AppExit::Success);
                }
                ButtonAction::Download { name, url } => {
                    if active_downloads.names.contains(name) {
                        continue;
                    }
                    active_downloads.names.insert(name.clone());

                    let tx = download_channel.tx.clone();
                    let name_clone = name.clone();
                    let url_clone = url.clone();

                    std::thread::spawn(move || {
                        let result = (|| {
                            let dest_path = get_game_download_path(&name_clone)
                                .ok_or_else(|| "Could not determine home directory".to_string())?;

                            // Ensure parent directory exists
                            if let Some(parent) = dest_path.parent() {
                                std::fs::create_dir_all(parent)
                                    .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                            }

                            // Remove existing directory if any
                            if dest_path.exists() {
                                std::fs::remove_dir_all(&dest_path)
                                    .map_err(|e| format!("Failed to clear existing folder: {}", e))?;
                            }

                            // Run git clone
                            let status = std::process::Command::new("git")
                                .args(["clone", &url_clone, dest_path.to_str().ok_or("Invalid path string")?])
                                .status()
                                .map_err(|e| format!("Failed to execute git clone: {}", e))?;

                            if !status.success() {
                                return Err(format!("git clone failed with exit status: {:?}", status.code()));
                            }

                            Ok(dest_path)
                        })();

                        match result {
                            Ok(path) => {
                                let _ = tx.send(DownloadResult {
                                    name: name_clone,
                                    path,
                                    result: Ok(())
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(DownloadResult {
                                    name: name_clone,
                                    path: PathBuf::new(),
                                    result: Err(e),
                                });
                            }
                        }
                    });

                    ui_events.send(UpdateUiEvent);
                }
            }
        }
    }
}

pub fn process_dialog_returns(
    dialog_channel: Res<DialogChannel>,
    mut config: ResMut<LauncherConfig>,
    running: Res<RunningProcesses>,
    mut ui_events: EventWriter<UpdateUiEvent>,
) {
    let rx = dialog_channel.rx.lock().unwrap();
    if let Ok(path) = rx.try_recv() {
        println!("Selected folder path: {}", path.display());

        // Resolve absolute path safely
        let absolute_path = std::fs::canonicalize(&path).unwrap_or(path);

        // Check if already in config
        if config.projects.iter().any(|p| p.path == absolute_path) {
            println!("Path {} is already in history", absolute_path.display());
            return;
        }

        let project_type = detect_project_type(&absolute_path);
        let status = if project_type == ProjectType::Invalid {
            println!("Warning: Path {} is not a standard Bevy game folder (no Cargo.toml or executable found).", absolute_path.display());
            ProjectStatus::Invalid
        } else {
            ProjectStatus::Idle
        };

        let name = absolute_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown Game".to_string());

        let new_project = Project {
            name,
            path: absolute_path.clone(),
            last_launched: None,
        };

        config.projects.push(new_project);
        save_config(&config);

        // Initialize status
        running.statuses.lock().unwrap().insert(absolute_path, status);

        ui_events.send(UpdateUiEvent);
    }
}

pub fn process_store_returns(
    store_channel: Res<StoreChannel>,
    mut store_games: ResMut<StoreGames>,
    mut ui_events: EventWriter<UpdateUiEvent>,
) {
    let rx = store_channel.rx.lock().unwrap();
    if let Ok(result) = rx.try_recv() {
        store_games.loading = false;
        match result {
            Ok(items) => {
                store_games.items = items;
                store_games.error = None;
            }
            Err(e) => {
                store_games.error = Some(e);
            }
        }
        ui_events.send(UpdateUiEvent);
    }
}

pub fn process_download_returns(
    download_channel: Res<DownloadChannel>,
    mut active_downloads: ResMut<ActiveDownloads>,
    mut config: ResMut<LauncherConfig>,
    running: Res<RunningProcesses>,
    mut ui_events: EventWriter<UpdateUiEvent>,
) {
    let rx = download_channel.rx.lock().unwrap();
    while let Ok(res) = rx.try_recv() {
        active_downloads.names.remove(&res.name);

        match res.result {
            Ok(()) => {
                println!("Successfully downloaded {} to {}", res.name, res.path.display());

                // Add to configuration if not present
                if !config.projects.iter().any(|p| p.path == res.path) {
                    let new_proj = Project {
                        name: res.name.clone(),
                        path: res.path.clone(),
                        last_launched: None,
                    };
                    config.projects.push(new_proj);
                    save_config(&config);
                }

                // Initialize status
                let project_type = detect_project_type(&res.path);
                let status = if project_type == ProjectType::Invalid {
                    ProjectStatus::Invalid
                } else {
                    ProjectStatus::Idle
                };
                running.statuses.lock().unwrap().insert(res.path.clone(), status);
            }
            Err(e) => {
                println!("Failed to download {}: {}", res.name, e);
            }
        }
        ui_events.send(UpdateUiEvent);
    }
}
