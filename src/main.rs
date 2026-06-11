use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::SystemTime;

mod config;
mod launcher;

use config::{load_config, save_config, LauncherConfig, Project};
use launcher::{detect_project_type, launch_project, ProjectType};

// Events
#[derive(Event)]
struct UpdateUiEvent;

// Components
#[derive(Component)]
struct ProjectListContainer;

#[derive(Component)]
struct ProjectCard;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ButtonType {
    SelectFolder,
    Launch,
    Stop,
    Delete,
}

#[derive(Component, Clone)]
enum ButtonAction {
    SelectFolder,
    Launch(PathBuf),
    Stop(PathBuf),
    Delete(PathBuf),
}

#[derive(Component)]
struct RotatingObject {
    axis: Vec3,
    speed: f32,
}

#[derive(Component)]
struct Particle {
    velocity: Vec3,
}

// Resources
#[derive(Resource)]
struct AppFonts {
    regular: Handle<Font>,
    semibold: Handle<Font>,
}

#[derive(Resource)]
struct DialogChannel {
    tx: Sender<PathBuf>,
    rx: Mutex<Receiver<PathBuf>>,
}

#[derive(Resource, Default)]
struct RunningProcesses {
    map: Mutex<HashMap<PathBuf, std::process::Child>>,
    statuses: Mutex<HashMap<PathBuf, ProjectStatus>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectStatus {
    Idle,
    Running,
    FailedLaunch,
    Invalid,
}

#[derive(Resource)]
struct InitialPath(Option<PathBuf>);

fn main() {
    // Check command line arguments for a game folder to launch directly
    let args: Vec<String> = std::env::args().collect();
    let initial_path = if args.len() > 1 {
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            Some(path)
        } else {
            println!("Warning: Provided path '{}' does not exist.", args[1]);
            None
        }
    } else {
        None
    };

    // Load initial configuration
    let config = load_config();

    // Create channel for the file dialog
    let (tx, rx) = channel::<PathBuf>();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "DeBevL - Bevy Game Launcher".to_string(),
                resolution: (800.0, 600.0).into(),
                resizable: false,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
        .insert_resource(config)
        .insert_resource(DialogChannel { tx, rx: Mutex::new(rx) })

        .insert_resource(RunningProcesses::default())
        .insert_resource(InitialPath(initial_path))
        .add_event::<UpdateUiEvent>()
        .add_systems(Startup, (
            setup_fonts,
            setup_ui,
            setup_3d_background,
            init_launcher,
            initial_launch_system,
        ).chain())
        .add_systems(Update, (
            process_dialog_returns,
            monitor_processes,
            handle_ui_buttons,
            button_system,
            card_hover_system,
            update_ui_list,
            animate_background,
        ))
        .run();
}

// Startup systems
fn setup_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let regular_bytes = include_bytes!("../assets/Roboto-Regular.ttf");
    let semibold_bytes = include_bytes!("../assets/Roboto-Medium.ttf");

    let regular = fonts.add(Font::try_from_bytes(regular_bytes.to_vec()).unwrap());
    let semibold = fonts.add(Font::try_from_bytes(semibold_bytes.to_vec()).unwrap());

    commands.insert_resource(AppFonts { regular, semibold });
}

fn setup_3d_background(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Lights
    commands.spawn((
        PointLight {
            intensity: 1500.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.0, 3.0, 3.0),
    ));

    commands.spawn((
        PointLight {
            intensity: 1000.0,
            color: Color::srgb(0.0, 0.8, 1.0),
            ..default()
        },
        Transform::from_xyz(-3.0, -3.0, 2.0),
    ));

    commands.spawn((
        PointLight {
            intensity: 800.0,
            color: Color::srgb(0.9, 0.1, 0.5),
            ..default()
        },
        Transform::from_xyz(3.0, -2.0, 1.0),
    ));

    // Large central torus
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(0.12, 1.4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.0, 0.6, 1.0, 0.4),
            perceptual_roughness: 0.1,
            metallic: 0.8,
            emissive: LinearRgba::new(0.0, 0.3, 0.8, 1.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
        RotatingObject {
            axis: Vec3::new(0.5, 1.0, 0.2).normalize(),
            speed: 0.3,
        },
    ));

    // Secondary inner torus (tilted)
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(0.08, 0.9))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.9, 0.1, 0.5, 0.3),
            perceptual_roughness: 0.2,
            metallic: 0.5,
            emissive: LinearRgba::new(0.6, 0.0, 0.3, 1.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
        RotatingObject {
            axis: Vec3::new(-0.8, 0.4, 0.5).normalize(),
            speed: -0.5,
        },
    ));

    // Glowing particle starfield
    for _ in 0..40 {
        let x = rand::random_range(-5.0..5.0);
        let y = rand::random_range(-4.0..4.0);
        let z = rand::random_range(-4.0..1.0);

        let vx = rand::random_range(-0.1..0.1);
        let vy = rand::random_range(-0.1..0.1);
        let vz = rand::random_range(-0.05..0.05);

        let color = if rand::random_bool(0.5) {
            Color::srgb(0.0, 0.8, 1.0)
        } else {
            Color::srgb(0.9, 0.1, 0.5)
        };

        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.03))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                emissive: LinearRgba::new(color.to_linear().red, color.to_linear().green, color.to_linear().blue, 1.0),
                ..default()
            })),
            Transform::from_xyz(x, y, z),
            Particle {
                velocity: Vec3::new(vx, vy, vz),
            },
        ));
    }
}

fn setup_ui(mut commands: Commands, fonts: Res<AppFonts>) {
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

                // "Select Folder" Button
                header.spawn((
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

            // Section Title
            panel.spawn(Node {
                margin: UiRect::bottom(Val::Px(12.0)),
                ..default()
            }).with_child((
                Text::new("Recent Bevy Games"),
                TextFont {
                    font: fonts.semibold.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
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

// System to initialize status map
fn init_launcher(
    config: Res<LauncherConfig>,
    running: Res<RunningProcesses>,
    mut ui_events: EventWriter<UpdateUiEvent>,
) {
    let mut statuses = running.statuses.lock().unwrap();
    for project in &config.projects {
        let status = if detect_project_type(&project.path) == ProjectType::Invalid {
            ProjectStatus::Invalid
        } else {
            ProjectStatus::Idle
        };
        statuses.insert(project.path.clone(), status);
    }
    ui_events.send(UpdateUiEvent);
}

// System to handle direct launching via CLI args
fn initial_launch_system(
    mut initial_path: ResMut<InitialPath>,
    mut config: ResMut<LauncherConfig>,
    running: Res<RunningProcesses>,
    mut ui_events: EventWriter<UpdateUiEvent>,
) {
    if let Some(path) = initial_path.0.take() {
        let Ok(absolute_path) = std::fs::canonicalize(&path) else {
            println!("Error: Path {} does not exist", path.display());
            return;
        };

        let project_type = detect_project_type(&absolute_path);
        if project_type == ProjectType::Invalid {
            println!("Error: Path {} does not contain a Cargo.toml or executable Bevy binary.", absolute_path.display());
            return;
        }

        // Add to history if not present
        if !config.projects.iter().any(|p| p.path == absolute_path) {
            let name = absolute_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Unknown Game".to_string());

            let new_proj = Project {
                name,
                path: absolute_path.clone(),
                last_launched: None,
            };
            config.projects.push(new_proj);
        }

        // Spawn it in the background
        let mut map = running.map.lock().unwrap();
        let mut statuses = running.statuses.lock().unwrap();

        match launch_project(&absolute_path) {
            Ok(child) => {
                println!("Launching {} in the background...", absolute_path.display());
                map.insert(absolute_path.clone(), child);
                statuses.insert(absolute_path.clone(), ProjectStatus::Running);

                // Update timestamp
                if let Some(proj) = config.projects.iter_mut().find(|p| p.path == absolute_path) {
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    proj.last_launched = Some(now);
                }
                save_config(&config);
            }
            Err(e) => {
                println!("Failed to launch project {}: {}", absolute_path.display(), e);
                statuses.insert(absolute_path.clone(), ProjectStatus::FailedLaunch);
            }
        }

        ui_events.send(UpdateUiEvent);
    }
}

// System to process folder dialog results
fn process_dialog_returns(
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

// System to monitor background processes
fn monitor_processes(
    running: Res<RunningProcesses>,
    mut ui_events: EventWriter<UpdateUiEvent>,
) {
    let mut map = running.map.lock().unwrap();
    let mut statuses = running.statuses.lock().unwrap();
    let mut changed = false;

    map.retain(|path, child| {
        match child.try_wait() {
            Ok(Some(status)) => {
                let status_val = if status.success() {
                    ProjectStatus::Idle
                } else {
                    ProjectStatus::FailedLaunch
                };
                statuses.insert(path.clone(), status_val);
                changed = true;
                false // Remove from running map
            }
            Ok(None) => {
                // Still running
                true
            }
            Err(e) => {
                println!("Error checking child process: {}", e);
                statuses.insert(path.clone(), ProjectStatus::Idle);
                changed = true;
                false
            }
        }
    });

    if changed {
        ui_events.send(UpdateUiEvent);
    }
}

// System to handle UI buttons
fn handle_ui_buttons(
    interaction_query: Query<(&Interaction, &ButtonAction), (Changed<Interaction>, With<Button>)>,
    mut config: ResMut<LauncherConfig>,
    running: Res<RunningProcesses>,
    dialog_channel: Res<DialogChannel>,
    mut ui_events: EventWriter<UpdateUiEvent>,
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
            }
        }
    }
}

// Button styling and animation
fn button_system(
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
        }
    }
}

// Card hover highlighting
fn card_hover_system(
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

// System to rebuild the project cards list when requested
fn update_ui_list(
    mut commands: Commands,
    fonts: Res<AppFonts>,
    config: Res<LauncherConfig>,
    running: Res<RunningProcesses>,
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
                Button, // Add Button so it has Interaction states
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
    });
}

// Background animation system
fn animate_background(
    time: Res<Time>,
    mut rotate_query: Query<(&mut Transform, &RotatingObject)>,
    mut particle_query: Query<(&mut Transform, &Particle), Without<RotatingObject>>,
) {
    let delta = time.delta_secs();

    // Rotate central shapes
    for (mut transform, rotating) in &mut rotate_query {
        transform.rotate(Quat::from_axis_angle(rotating.axis, rotating.speed * delta));
    }

    // Drift particles
    for (mut transform, particle) in &mut particle_query {
        transform.translation += particle.velocity * delta;

        // Wrap around bounds
        if transform.translation.x > 6.0 {
            transform.translation.x = -6.0;
        } else if transform.translation.x < -6.0 {
            transform.translation.x = 6.0;
        }

        if transform.translation.y > 5.0 {
            transform.translation.y = -5.0;
        } else if transform.translation.y < -5.0 {
            transform.translation.y = 5.0;
        }

        if transform.translation.z > 2.0 {
            transform.translation.z = -4.0;
        } else if transform.translation.z < -4.0 {
            transform.translation.z = 2.0;
        }
    }
}
