// Tenstorrent RGB Lighting Controller
// Monitors Tenstorrent hardware temperature/power and controls RGB lighting via OpenRGB

use tt_qb_lights::{config, monitoring, rgb};

use anyhow::{Context, Result};
use clap::Parser;

/// Macro to warn only once (avoid log spam)
/// Must be defined before first use
macro_rules! warn_once {
    ($($arg:tt)*) => {{
        use std::sync::Once;
        static WARN_ONCE: Once = Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!($($arg)*);
        });
    }};
}
use monitoring::{sensors::SensorsMonitor, tenstorrent::TtSmiMonitor, HardwareMonitor};
use rgb::{color_mapping::ColorMapper, openrgb_cli::OpenRgbCliController, RgbController};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Tenstorrent RGB Lighting Controller
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file (if not specified, searches standard locations)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Initialize default configuration file at ~/.config/tt-qb-lights/config.toml
    #[arg(long)]
    init: bool,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Dry run mode (don't actually control RGB lights)
    #[arg(long)]
    dry_run: bool,

    /// Single poll (print metrics and exit, don't run continuously)
    #[arg(short = 's', long)]
    single_shot: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --init flag
    if args.init {
        let config_path = config::Config::init_default_config()?;
        println!("Created default configuration at: {}", config_path.display());
        println!("\nEdit this file to customize your RGB lighting settings.");
        println!("Then run: tt-qb-lights");
        return Ok(());
    }

    // Initialize logging
    init_logging(&args)?;

    info!("Starting Tenstorrent RGB Lighting Controller");

    // Load configuration
    let config = config::Config::load(args.config.as_deref())
        .with_context(|| {
            if let Some(ref path) = args.config {
                format!("Failed to load config from {}", path.display())
            } else {
                "Failed to load config from standard locations".to_string()
            }
        })?;

    if let Some(ref path) = args.config {
        info!("Loaded configuration from {}", path.display());
    } else {
        info!("Loaded configuration from standard location");
    }
    info!(
        "Monitoring source: {:?}, poll interval: {}ms",
        config.monitoring.source, config.monitoring.poll_interval_ms
    );

    // Initialize hardware monitor
    let monitor: Box<dyn HardwareMonitor> = match config.monitoring.source {
        config::MonitoringSource::TtSmi => {
            Box::new(TtSmiMonitor::new().context("Failed to initialize tt-smi monitor")?)
        }
        config::MonitoringSource::LmSensors => {
            Box::new(SensorsMonitor::new().context("Failed to initialize sensors monitor")?)
        }
    };

    info!("Initialized {} monitor", monitor.source_name());

    // Test initial poll
    let initial_metrics = monitor.poll_metrics()?;
    info!("Discovered {} Tenstorrent device(s)", initial_metrics.len());
    for metrics in &initial_metrics {
        info!(
            "  Device {}: {} ({}) - Temp: {:.1}°C, Power: {:.1}W",
            metrics.bus_id, metrics.board_type, metrics.architecture, metrics.max_temp, metrics.power_watts
        );
    }

    // Single-shot mode: just print metrics and exit
    if args.single_shot {
        info!("Single-shot mode: printing metrics and exiting");
        print_metrics_table(&initial_metrics);
        return Ok(());
    }

    // Initialize RGB controller (unless dry run)
    let mut rgb_controller: Option<OpenRgbCliController> = if args.dry_run {
        warn!("Dry run mode: RGB control disabled");
        None
    } else {
        match OpenRgbCliController::connect(
            &config.openrgb.server_host,
            config.openrgb.server_port,
            &config.openrgb.device_name,
        ) {
            Ok(client) => {
                info!("Connected to OpenRGB device: {}", client.device_name());
                Some(client)
            }
            Err(e) => {
                error!("Failed to connect to OpenRGB: {}", e);
                error!("Make sure OpenRGB is running with SDK server enabled:");
                error!("  openrgb --server");
                return Err(e);
            }
        }
    };

    // Initialize color mapper
    let color_mapper = ColorMapper::new(config.get_active_scheme())
        .context("Failed to initialize color mapper")?;

    info!(
        "Using color scheme '{}' ({}°C to {}°C)",
        config.color_mapping.scheme,
        color_mapper.min_temp(),
        color_mapper.max_temp()
    );

    // Main monitoring loop
    let poll_interval = Duration::from_millis(config.monitoring.poll_interval_ms);
    let mut loop_count = 0u64;
    let mut last_log_time = Instant::now();
    let mut last_rgb_color: Option<(rgb::RgbColor, f32)> = None; // Track last color sent
    let mut last_rgb_update = Instant::now(); // Track last RGB update time
    let min_rgb_update_interval = Duration::from_secs(5); // Minimum 5 seconds between updates

    info!("Starting main monitoring loop (Ctrl+C to stop)");

    loop {
        let loop_start = Instant::now();

        // Poll hardware metrics
        match monitor.poll_metrics() {
            Ok(metrics) => {
                loop_count += 1;

                // Find hottest device
                let hottest = metrics
                    .iter()
                    .max_by(|a, b| a.max_temp.partial_cmp(&b.max_temp).unwrap())
                    .unwrap();

                // Calculate color based on temperature
                let color = color_mapper.map_temperature(hottest.max_temp);

                // Calculate brightness based on power utilization
                let brightness = if config.effects.enable_power_brightness {
                    config.effects.min_brightness
                        + (config.effects.max_brightness - config.effects.min_brightness)
                            * hottest.power_utilization
                } else {
                    config.effects.max_brightness
                };

                // Apply pulsing effect if overheating
                let final_brightness = if config.effects.enable_warning_pulse
                    && hottest.max_temp >= config.effects.warning_temp_threshold
                {
                    // Pulse between 50% and 100% of calculated brightness
                    let pulse_phase = (loop_count as f32 * 0.1).sin() * 0.5 + 0.5;
                    brightness * (0.5 + pulse_phase * 0.5)
                } else {
                    brightness
                };

                // Update RGB lights only if:
                // 1. Color/brightness changed significantly AND
                // 2. Enough time has passed since last update (prevent flickering)
                let color_changed = if let Some((last_color, last_brightness)) = last_rgb_color {
                    // Check if color changed by at least 5 units in any channel
                    // or brightness changed by more than 5%
                    (color.r as i16 - last_color.r as i16).abs() > 5
                        || (color.g as i16 - last_color.g as i16).abs() > 5
                        || (color.b as i16 - last_color.b as i16).abs() > 5
                        || (final_brightness - last_brightness).abs() > 0.05
                } else {
                    true // First update
                };

                let enough_time_passed = last_rgb_update.elapsed() >= min_rgb_update_interval;
                let should_update = color_changed && enough_time_passed;

                if should_update {
                    if let Some(controller) = rgb_controller.as_mut() {
                        match config.openrgb.zone_strategy {
                            config::ZoneStrategy::Unified => {
                                // All zones show the same color
                                if let Err(e) = controller.set_all(color, final_brightness) {
                                    error!("Failed to update RGB lights: {}", e);
                                } else {
                                    last_rgb_color = Some((color, final_brightness));
                                    last_rgb_update = Instant::now();
                                    info!("Updated RGB to #{:02X}{:02X}{:02X} @ {:.0}%",
                                        color.r, color.g, color.b, final_brightness * 100.0);
                                }
                            }
                            config::ZoneStrategy::PerDevice => {
                                // TODO: Implement per-device zone mapping
                                warn_once!("Per-device zone strategy not yet implemented, using unified");
                                if let Err(e) = controller.set_all(color, final_brightness) {
                                    error!("Failed to update RGB lights: {}", e);
                                } else {
                                    last_rgb_color = Some((color, final_brightness));
                                    last_rgb_update = Instant::now();
                                }
                            }
                            config::ZoneStrategy::Gradient => {
                                // TODO: Implement gradient across all LEDs
                                warn_once!("Gradient zone strategy not yet implemented, using unified");
                                if let Err(e) = controller.set_all(color, final_brightness) {
                                    error!("Failed to update RGB lights: {}", e);
                                } else {
                                    last_rgb_color = Some((color, final_brightness));
                                    last_rgb_update = Instant::now();
                                }
                            }
                        }
                    }
                }

                // Periodic status logging (every 10 seconds)
                if last_log_time.elapsed() >= Duration::from_secs(10) {
                    info!(
                        "Status: {:.1}°C (max) | {:.1}W | RGB: #{:02X}{:02X}{:02X} @ {:.0}%",
                        hottest.max_temp,
                        hottest.power_watts,
                        color.r,
                        color.g,
                        color.b,
                        final_brightness * 100.0
                    );
                    last_log_time = Instant::now();
                }
            }
            Err(e) => {
                error!("Failed to poll metrics: {}", e);
                // Continue loop despite error (might be transient)
            }
        }

        // Sleep until next poll interval or handle Ctrl+C
        let elapsed = loop_start.elapsed();
        let sleep_duration = if elapsed < poll_interval {
            poll_interval - elapsed
        } else {
            warn!("Loop took longer than poll interval: {:?}", elapsed);
            Duration::from_millis(10) // Tiny sleep to avoid busy loop
        };

        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => {
                // Normal sleep completed, continue loop
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal, exiting gracefully");
                break;
            }
        }
    }

    // Cleanup: restore original RGB state
    if let Some(controller) = rgb_controller {
        info!("Restoring original RGB configuration");
        controller.restore_original();
    }

    info!("Shutdown complete");
    Ok(())
}

/// Initialize logging based on configuration and CLI args
fn init_logging(args: &Args) -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let log_level = if args.debug { "debug" } else { "info" };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("tt_qb_lights={},warn", log_level)));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    Ok(())
}

/// Print metrics in a formatted table
fn print_metrics_table(metrics: &[monitoring::DeviceMetrics]) {
    println!("\n{:=<80}", "");
    println!("{:^80}", "Tenstorrent Device Metrics");
    println!("{:=<80}", "");
    println!(
        "{:<15} {:<10} {:<12} {:>10} {:>10} {:>8}",
        "Bus ID", "Board", "Architecture", "Temp (°C)", "Power (W)", "Fan RPM"
    );
    println!("{:-<80}", "");

    for m in metrics {
        println!(
            "{:<15} {:<10} {:<12} {:>10.1} {:>10.1} {:>8}",
            m.bus_id,
            m.board_type,
            m.architecture,
            m.max_temp,
            m.power_watts,
            m.fan_rpm
        );

        if !m.gddr_temps.is_empty() {
            println!("  GDDR temps: {:?}", m.gddr_temps);
        }
    }

    println!("{:=<80}\n", "");
}
