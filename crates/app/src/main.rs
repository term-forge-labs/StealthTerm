#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::{NativeOptions, run_native};
use egui::ViewportBuilder;
use egui_wgpu;
// use tracing_subscriber::EnvFilter;

mod app;
mod event;

use app::StealthTermApp;

fn main() -> eframe::Result<()> {
    // Logging disabled — uncomment to enable file logging
    // let log_file = std::fs::File::create("stealthterm.log").expect("Failed to create log file");
    // tracing_subscriber::fmt()
    //     .with_writer(log_file)
    //     .with_env_filter(
    //         EnvFilter::try_from_default_env()
    //             .unwrap_or_else(|_| EnvFilter::new("stealthterm=info,warn")),
    //     )
    //     .init();

    // tracing::info!("StealthTerm starting...");

    // Load window icon — use PNG format icon
    let icon_data = include_bytes!("../../../assets/icon.png");
    let icon = match image::load_from_memory(icon_data) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            Some(egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            })
        }
        Err(e) => {
            tracing::warn!("Failed to load icon: {}", e);
            None
        }
    };

    let mut viewport = ViewportBuilder::default()
        .with_title("StealthTerm")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([600.0, 400.0])
        .with_decorations(false)
        .with_transparent(false);

    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: Some(1), // low latency
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew {
                // Prefer high-performance discrete GPU
                power_preference: wgpu::PowerPreference::HighPerformance,
                instance_descriptor: wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::all(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    run_native(
        "StealthTerm",
        options,
        Box::new(|cc| {
            // Install image loaders (for color emoji)
            egui_extras::install_image_loaders(&cc.egui_ctx);

            // Add Chinese font support
            let mut fonts = egui::FontDefinitions::default();

            // Embed JetBrains Mono terminal-specific font
            fonts.font_data.insert(
                "JetBrainsMono".to_owned(),
                egui::FontData::from_static(include_bytes!("../../../resources/fonts/JetBrainsMono-Regular.ttf")).into(),
            );
            fonts.font_data.insert(
                "JetBrainsMono-Bold".to_owned(),
                egui::FontData::from_static(include_bytes!("../../../resources/fonts/JetBrainsMono-Bold.ttf")).into(),
            );
            fonts.font_data.insert(
                "JetBrainsMono-Italic".to_owned(),
                egui::FontData::from_static(include_bytes!("../../../resources/fonts/JetBrainsMono-Italic.ttf")).into(),
            );
            fonts.font_data.insert(
                "JetBrainsMono-BoldItalic".to_owned(),
                egui::FontData::from_static(include_bytes!("../../../resources/fonts/JetBrainsMono-BoldItalic.ttf")).into(),
            );

            // JetBrains Mono as preferred Monospace font (before Hack)
            fonts.families.get_mut(&egui::FontFamily::Monospace)
                .unwrap()
                .insert(0, "JetBrainsMono".to_owned());

            // Register separate font families for terminal attribute-based selection
            fonts.families.insert(
                egui::FontFamily::Name("TermBold".into()),
                vec!["JetBrainsMono-Bold".to_owned(), "JetBrainsMono".to_owned()],
            );
            fonts.families.insert(
                egui::FontFamily::Name("TermItalic".into()),
                vec!["JetBrainsMono-Italic".to_owned(), "JetBrainsMono".to_owned()],
            );
            fonts.families.insert(
                egui::FontFamily::Name("TermBoldItalic".into()),
                vec!["JetBrainsMono-BoldItalic".to_owned(), "JetBrainsMono".to_owned()],
            );

            // Add Chinese font as fallback
            let chinese_font_loaded = {
                #[cfg(target_os = "windows")]
                {
                    // Windows: use monospace Chinese font
                    if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
                        fonts.font_data.insert("chinese".to_owned(), egui::FontData::from_owned(font_data).into());
                        true
                    } else {
                        false
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    if let Ok(font_data) = std::fs::read("/System/Library/Fonts/PingFang.ttc") {
                        fonts.font_data.insert("chinese".to_owned(), egui::FontData::from_owned(font_data).into());
                        true
                    } else {
                        false
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    let paths = [
                        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
                        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
                        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
                    ];
                    let mut loaded = false;
                    for path in &paths {
                        if let Ok(font_data) = std::fs::read(path) {
                            fonts.font_data.insert("chinese".to_owned(), egui::FontData::from_owned(font_data).into());
                            loaded = true;
                            break;
                        }
                    }
                    loaded
                }
            };

            if chinese_font_loaded {
                // Chinese font placed last as fallback
                for family in [
                    egui::FontFamily::Proportional,
                    egui::FontFamily::Monospace,
                    egui::FontFamily::Name("TermBold".into()),
                    egui::FontFamily::Name("TermItalic".into()),
                    egui::FontFamily::Name("TermBoldItalic".into()),
                ] {
                    if let Some(list) = fonts.families.get_mut(&family) {
                        list.push("chinese".to_owned());
                    }
                }
            }

            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(StealthTermApp::new(cc)))
        }),
    )
}
