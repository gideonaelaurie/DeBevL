use bevy::prelude::*;
use std::path::PathBuf;
use std::time::SystemTime;

use bevui_28::{
    _28bevUI, setup_fonts, setup_ui, LauncherConfig, Project, RunningProcesses, ProjectStatus,
    ProjectType, detect_project_type, launch_project, load_config, save_config,
    UpdateUiEvent,
};

#[derive(Component)]
struct RotatingObject {
    axis: Vec3,
    speed: f32,
}

#[derive(Component)]
struct Particle {
    velocity: Vec3,
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

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "DeBevL - Bevy Game Launcher".to_string(),
                mode: bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(_28bevUI)
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
        .insert_resource(config)
        .insert_resource(InitialPath(initial_path))
        .add_systems(Startup, (
            setup_fonts,
            setup_ui,
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
    mut window_query: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
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

    if let Ok(mut window) = window_query.get_single_mut() {
        let is_any_running = !map.is_empty();
        let target_visible = !is_any_running;
        if window.visible != target_visible {
            window.visible = target_visible;
        }
    }

    if changed {
        ui_events.send(UpdateUiEvent);
    }
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
