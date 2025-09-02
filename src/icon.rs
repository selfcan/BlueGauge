use crate::{
    config::{Config, TrayIconSource},
    theme::SystemTheme,
};

use anyhow::{Context, Result, anyhow};
use piet_common::{
    Color, Device, FontFamily, ImageFormat, RenderContext, Text, TextLayout, TextLayoutBuilder,
};
use tray_icon::Icon;

const LOGO_DATA: &[u8] = include_bytes!("../assets/logo.ico");

pub fn load_app_icon() -> Result<Icon> {
    load_icon(LOGO_DATA).map_err(|e| anyhow!("Failed to load app icon - {e}"))
}

pub fn load_icon(icon_date: &[u8]) -> Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(icon_date)
            .with_context(|| "Failed to open icon path")?
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).with_context(|| "Failed to crate the logo")
}

pub fn load_battery_icon(
    config: &Config,
    bluetooth_battery: u8,
    bluetooth_status: bool,
) -> Result<Icon> {
    let tray_icon_source = config.tray_options.tray_icon_source.lock().unwrap().clone();

    match tray_icon_source {
        TrayIconSource::App => load_app_icon(),
        TrayIconSource::BatteryCustom { .. } => get_icon_from_custom(bluetooth_battery),
        TrayIconSource::BatteryFont {
            address: _,
            font_name,
            font_color,
            font_size,
        } => {
            let should_icon_connect_color = font_color
                .as_ref()
                .is_some_and(|c| c.eq("ConnectColor"))
                .then_some(bluetooth_status);

            get_icon_from_font(
                bluetooth_battery,
                &font_name,
                font_color,
                font_size,
                should_icon_connect_color,
            )
        }
    }
}

fn get_icon_from_custom(battery_level: u8) -> Result<Icon> {
    let custom_battery_icon_path = std::env::current_exe()
        .map(|exe_path| exe_path.with_file_name("assets"))
        .and_then(|icon_dir| {
            let default_icon_path = icon_dir.join(format!("{battery_level}.png"));
            if default_icon_path.is_file() {
                return Ok(default_icon_path);
            }
            let theme_icon_path = match SystemTheme::get() {
                SystemTheme::Light => icon_dir.join(format!("light\\{battery_level}.png")),
                SystemTheme::Dark => icon_dir.join(format!("dark\\{battery_level}.png")),
            };
            if theme_icon_path.is_file() {
                return Ok(theme_icon_path);
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to find {battery_level} default/theme PNG in Bluegauge directory"),
            ))
        })?;

    let icon_data = std::fs::read(custom_battery_icon_path)?;

    load_icon(&icon_data)
}

fn get_icon_from_font(
    battery_level: u8,
    font_name: &str,
    font_color: Option<String>,
    font_size: Option<u8>,
    should_icon_connect_color: Option<bool>,
) -> Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = render_battery_font_icon(
        battery_level,
        font_name,
        font_color,
        font_size,
        should_icon_connect_color,
    )?;
    Icon::from_rgba(icon_rgba, icon_width, icon_height)
        .map_err(|e| anyhow!("Failed to get Icon - {e}"))
}

fn render_battery_font_icon(
    battery_level: u8,
    font_name: &str,
    font_color: Option<String>, // 格式：#123456、#123456FF
    font_size: Option<u8>,
    should_icon_connect_color: Option<bool>,
) -> Result<(Vec<u8>, u32, u32)> {
    let indicator = battery_level.to_string();

    let width = 64;
    let height = 64;
    let font_size = font_size.and_then(|s| s.ne(&64).then_some(s as f64));
    let font_color = if let Some(should) = should_icon_connect_color {
        if should {
            "#4fc478".to_owned()
        } else {
            "#fe6666ff".to_owned()
        }
    } else {
        font_color
            .and_then(|c| c.ne("FollowSystemTheme").then_some(c))
            .unwrap_or_else(|| SystemTheme::get().get_font_color())
    };

    let mut device = Device::new().map_err(|e| anyhow!("Failed to get Device - {e}"))?;

    let mut bitmap_target = device
        .bitmap_target(width, height, 1.0)
        .map_err(|e| anyhow!("Failed to create a new bitmap target. - {e}"))?;

    let mut piet = bitmap_target.render_context();

    // Dynamically calculated font size
    let mut layout;
    let text = piet.text();

    let mut fs = match (font_size, battery_level) {
        (_, 100) => 42.0,
        (Some(size), _) => size,
        (None, b) if b < 10 => 70.0,
        (None, _) => 64.0,
    };

    if battery_level == 100 || font_size.is_none() {
        while {
            layout = build_text_layout(text, &indicator, font_name, fs, &font_color)?;
            !(layout.size().width > width as f64 || layout.size().height > height as f64)
        } {
            fs += 2.0;
        }
    } else {
        layout = build_text_layout(text, &indicator, font_name, fs, &font_color)?;
    }

    let (x, y) = (
        (width as f64 - layout.size().width) / 2.0,
        (height as f64 - layout.size().height) / 2.0,
    );

    piet.draw_text(&layout, (x, y));
    piet.finish().map_err(|e| anyhow!("{e}"))?;
    drop(piet);

    let image_buf = bitmap_target.to_image_buf(ImageFormat::RgbaPremul).unwrap();

    Ok((
        image_buf.raw_pixels().to_vec(),
        image_buf.width() as u32,
        image_buf.height() as u32,
    ))
}

fn build_text_layout(
    text: &mut piet_common::D2DText,
    indicator: &str,
    font_name: &str,
    font_size: f64,
    font_color: &str,
) -> Result<piet_common::D2DTextLayout> {
    text.new_text_layout(indicator.to_string())
        .font(FontFamily::new_unchecked(font_name), font_size)
        .text_color(Color::from_hex_str(font_color)?)
        .build()
        .map_err(|e| anyhow!("Failed to build text layout - {e}"))
}
