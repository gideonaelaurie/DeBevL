use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::SystemTime;

mod config;
mod launcher;
mod ui_28;

use config::{load_config, save_config, LauncherConfig, Project};
use launcher::{detect_project_type, launch_project, ProjectType};
use ui_28::{_28ui, UpdateUiEvent};

use serde::Deserialize;

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

#[derive(Debug, Clone)]
struct StoreGameItem {
    name: String,
    owner: String,
    description: String,
    stars: u32,
    url: String,
}

#[derive(Resource, Default)]
struct StoreGames {
    items: Vec<StoreGameItem>,
    loading: bool,
    error: Option<String>,
}

#[derive(Resource)]
struct StoreChannel {
    rx: Mutex<Receiver<Result<Vec<StoreGameItem>, String>>>,
}

struct DownloadResult {
    name: String,
    path: PathBuf,
    result: Result<(), String>,
}

#[derive(Resource)]
struct DownloadChannel {
    tx: Sender<DownloadResult>,
    rx: Mutex<Receiver<DownloadResult>>,
}

#[derive(Resource, Default)]
struct ActiveDownloads {
    names: std::collections::HashSet<String>,
}

fn get_game_download_path(game_name: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let mut path = PathBuf::from(home);
    path.push(".debevl");
    path.push("apps");
    path.push("games");
    path.push(game_name);
    Some(path)
}

#[derive(Debug, Clone, Deserialize)]
struct GithubOwner {
    login: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRepoItem {
    name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u32,
    owner: GithubOwner,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubSearchResponse {
    items: Vec<GithubRepoItem>,
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

    // Create channel for GitHub store
    let (store_tx, store_rx) = channel::<Result<Vec<StoreGameItem>, String>>();
    fetch_store_games(store_tx);

    // Create channel for game downloads
    let (download_tx, download_rx) = channel::<DownloadResult>();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "DeBevL - Bevy Game Launcher".to_string(),
                mode: bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(_28ui)
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
        .insert_resource(config)
        .insert_resource(DialogChannel { tx, rx: Mutex::new(rx) })
        .insert_resource(DownloadChannel { tx: download_tx, rx: Mutex::new(download_rx) })
        .insert_resource(ActiveDownloads::default())
        .insert_resource(StoreGames { loading: true, ..default() })
        .insert_resource(StoreChannel { rx: Mutex::new(store_rx) })
        .insert_resource(RunningProcesses::default())
        .insert_resource(InitialPath(initial_path))
        .add_systems(Startup, (
            setup_fonts,
            ui_28::setup_ui,
            setup_3d_background,
            init_launcher,
            initial_launch_system,
        ).chain())
        .add_systems(Update, (
            monitor_processes,
            animate_background,
        ))
        .run();
}

fn fetch_store_games(tx: Sender<Result<Vec<StoreGameItem>, String>>) {
    std::thread::spawn(move || {
        let url = "https://api.github.com/search/repositories?q=topic:bevy+topic:game&sort=stars&order=desc";
        let request = ureq::get(url).set("User-Agent", "DeBevL-Game-Launcher");
        
        match request.call() {
            Ok(response) => {
                match response.into_json::<GithubSearchResponse>() {
                    Ok(search_res) => {
                        let mut items = Vec::new();
                        // Query the top 5 Bevy games
                        for item in search_res.items.into_iter().take(5) {
                            let owner = item.owner.login.clone();
                            let repo_name = item.name.clone();
                            let readme_url = format!("https://api.github.com/repos/{}/{}/readme", owner, repo_name);
                            
                            // Fallback to repository description if README is missing or rate-limited
                            let mut description = item.description.clone().unwrap_or_default();
                            
                            if let Ok(readme_resp) = ureq::get(&readme_url)
                                .set("User-Agent", "DeBevL-Game-Launcher")
                                .set("Accept", "application/vnd.github.raw")
                                .call()
                            {
                                if let Ok(readme_text) = readme_resp.into_string() {
                                    let cleaned = clean_readme(&readme_text);
                                    if !cleaned.is_empty() {
                                        description = cleaned;
                                    }
                                }
                            }
                            
                            if description.is_empty() {
                                description = "No description provided.".to_string();
                            }

                            items.push(StoreGameItem {
                                name: repo_name,
                                owner,
                                description,
                                stars: item.stargazers_count,
                                url: item.html_url,
                            });
                        }
                        let _ = tx.send(Ok(items));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(format!("JSON Parse Error: {}", e)));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Err(format!("Network Error: {}", e)));
            }
        }
    });
}

fn clean_readme(readme: &str) -> String {
    let mut clean: String = readme
        .lines()
        .map(|line| line.trim())
        // Filter out empty lines, markdown headers, links, badges, images, and code blocks
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with('[')
                && !line.starts_with('<')
                && !line.starts_with('!')
                && !line.starts_with("```")
        })
        .collect::<Vec<_>>()
        .join(" ");

    if clean.len() > 180 {
        clean.truncate(177);
        clean.push_str("...");
    }

    clean
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

// UI setup has been moved to ui_28 plugin

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

// Systems handle_ui_buttons, button_system, card_hover_system, and update_ui_list have been moved to ui_28 plugin

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

// Systems process_store_returns, update_tab_buttons, and process_download_returns have been moved to ui_28 plugin

