use crate::{
    config::{ASSETS_PATH, Config, TrayIconStyle},
    notify::notify,
    theme::SystemTheme,
};

use anyhow::{Context, Result, anyhow};
use piet_common::{
    Color, D2DText, D2DTextLayout, Device, FontFamily, ImageFormat, LineCap, RenderContext,
    StrokeStyle, Text, TextLayout, TextLayoutBuilder,
};
use tray_icon::Icon;

const LOGO_DATA: &[u8] = include_bytes!("../../assets/logo.ico");

pub fn load_icon(icon_date: &[u8]) -> Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(icon_date)
            .map_err(|e| anyhow!("Failed to load icon - {e}"))?
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).with_context(|| "Failed to crate the logo")
}

pub fn load_app_icon() -> Result<Icon> {
    load_icon(LOGO_DATA).map_err(|e| anyhow!("Failed to load app icon - {e}"))
}

pub fn load_tray_icon(config: &Config, battery_level: u8, bluetooth_status: bool) -> Result<Icon> {
    let tray_icon_style = config.tray_options.tray_icon_style.lock().unwrap().clone();

    match tray_icon_style {
        TrayIconStyle::App => load_app_icon(),
        TrayIconStyle::BatteryCustom { .. } => load_custom_icon(battery_level),
        TrayIconStyle::BatteryIcon {
            address: _,
            color_scheme,
            font_size,
            ..
        } => {
            let is_low_battery = battery_level <= config.get_low_battery();

            let is_connect_color = color_scheme.is_connect_color().then_some(bluetooth_status);

            load_battery_icon(battery_level, is_low_battery, font_size, is_connect_color)
        }
        TrayIconStyle::BatteryNumber {
            address: _,
            color_scheme,
            font_name,
            font_color,
            font_size,
        } => {
            let is_connect_color = color_scheme.is_connect_color().then_some(bluetooth_status);

            load_number_icon(
                battery_level,
                &font_name,
                font_color,
                font_size,
                is_connect_color,
            )
        }
        TrayIconStyle::BatteryRing {
            address: _,
            color_scheme,
            highlight_color,
            background_color,
        } => {
            let is_low_battery = battery_level <= config.get_low_battery();

            let is_connect_color = color_scheme.is_connect_color().then_some(bluetooth_status);

            load_ring_icon(
                battery_level,
                is_low_battery,
                highlight_color,
                background_color,
                is_connect_color,
            )
        }
    }
}

fn load_custom_icon(battery_level: u8) -> Result<Icon> {
    let custom_battery_icon_path = || {
        let icon_dir = &ASSETS_PATH;
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
    };

    let icon_data = std::fs::read(custom_battery_icon_path()?)?;

    load_icon(&icon_data)
}

fn load_battery_icon(
    battery_level: u8,
    is_low_battery: bool,
    font_size: Option<u8>,
    is_connect_color: Option<bool>,
) -> Result<Icon> {
    let (icon_rgba, icon_width, icon_height) =
        render_battery_icon(battery_level, is_low_battery, font_size, is_connect_color)
            .inspect_err(|_| {
                notify("Please install 'Segoe Fluent Icons' Font");
            })?;
    Icon::from_rgba(icon_rgba, icon_width, icon_height)
        .map_err(|e| anyhow!("Failed to get Number Icon - {e}"))
}

fn load_number_icon(
    battery_level: u8,
    font_name: &str,
    font_color: Option<String>,
    font_size: Option<u8>,
    is_connect_color: Option<bool>,
) -> Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = render_number_icon(
        battery_level,
        font_name,
        font_color,
        font_size,
        is_connect_color,
    )?;
    Icon::from_rgba(icon_rgba, icon_width, icon_height)
        .map_err(|e| anyhow!("Failed to get Number Icon - {e}"))
}

pub fn load_ring_icon(
    battery_level: u8,
    is_low_battery: bool,
    highlight_color: Option</* Hex color */ String>,
    background_color: Option</* Hex color */ String>,
    is_connect_color: Option<bool>,
) -> Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = render_ring_icon(
        battery_level,
        is_low_battery,
        highlight_color,
        background_color,
        is_connect_color,
    )?;
    Icon::from_rgba(icon_rgba, icon_width, icon_height)
        .map_err(|e| anyhow!("Failed to get Icon - {e}"))
}

fn render_battery_icon(
    battery_level: u8,
    is_low_battery: bool,
    font_size: Option<u8>,
    is_connect_color: Option<bool>,
) -> Result<(Vec<u8>, u32, u32)> {
    if !std::path::Path::new(r"C:\WINDOWS\FONTS\SEGOEICONS.TTF").try_exists()? {
        return Err(anyhow!("Segoe Fluent Icons font not found"));
    }

    let indicator = if battery_level == 0 {
        String::from('\u{e850}')
    } else {
        const ICONS: [char; 11] = [
            '\u{e851}', // 1-10
            '\u{e852}', // 11-20
            '\u{e853}', // 21-30
            '\u{e854}', // 31-40
            '\u{e855}', // 41-50
            '\u{e856}', // 51-60
            '\u{e857}', // 61-70
            '\u{e858}', // 71-80
            '\u{e859}', // 81-90
            '\u{e83f}', // 91-100
            '\u{e83f}', // fallback (101+)
        ];
        ICONS[((battery_level - 1) / 10).min(10) as usize].to_string()
    };

    let width = 64;
    let height = 64;
    let font_name = "Segoe Fluent Icons";

    let font_size = font_size.map(|s| s as f64).unwrap_or(64.0);

    let font_color = {
        let base_color = if is_low_battery {
            Color::from_rgba32_u32(0xFE6666FF)
        } else {
            SystemTheme::get().get_font_color()
        };

        match is_connect_color {
            Some(true) => base_color,
            Some(false) => base_color.with_alpha(0.6),
            None => base_color,
        }
    };

    let mut device = Device::new().map_err(|e| anyhow!("Failed to get Device - {e}"))?;
    let mut bitmap_target = device
        .bitmap_target(width, height, 1.0)
        .map_err(|e| anyhow!("Failed to create a new bitmap target. - {e}"))?;
    let mut piet = bitmap_target.render_context();
    let text = piet.text();
    let layout = build_text_layout(text, &indicator, font_name, font_size, font_color)?;

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

fn render_number_icon(
    battery_level: u8,
    font_name: &str,
    font_color: Option</* Hex color */ String>,
    font_size: Option<u8>,
    is_connect_color: Option<bool>,
) -> Result<(Vec<u8>, u32, u32)> {
    let indicator = battery_level.to_string();

    let width = 64;
    let height = 64;

    let font_name = if font_name.trim().is_empty() {
        "Arial"
    } else {
        font_name
    };

    let font_size = font_size.and_then(|s| s.ne(&64).then_some(s as f64));

    let font_color = if let Some(should) = is_connect_color {
        if should {
            Color::from_rgba32_u32(0x4FC478FF)
        } else {
            Color::from_rgba32_u32(0xFE6666FF)
        }
    } else {
        font_color
            .and_then(|c| Color::from_hex_str(&c).ok())
            .unwrap_or_else(|| SystemTheme::get().get_font_color())
    };

    let mut device = Device::new().map_err(|e| anyhow!("Failed to get Device - {e}"))?;
    let mut bitmap_target = device
        .bitmap_target(width, height, 1.0)
        .map_err(|e| anyhow!("Failed to create a new bitmap target. - {e}"))?;
    let mut piet = bitmap_target.render_context();
    let mut layout;
    let text = piet.text();

    // Dynamically calculated font size
    let mut fs = match (font_size, battery_level) {
        (_, 100) => 42.0,
        (Some(size), _) => size,
        (None, b) if b < 10 => 70.0,
        (None, _) => 64.0,
    };

    if battery_level == 100 || font_size.is_none() {
        while {
            layout = build_text_layout(text, &indicator, font_name, fs, font_color)?;
            !(layout.size().width > width as f64 || layout.size().height > height as f64)
        } {
            fs += 2.0;
        }
    } else {
        layout = build_text_layout(text, &indicator, font_name, fs, font_color)?;
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
    text: &mut D2DText,
    indicator: &str,
    font_name: &str,
    font_size: f64,
    font_color: Color,
) -> Result<D2DTextLayout> {
    text.new_text_layout(indicator.to_string())
        .font(FontFamily::new_unchecked(font_name), font_size)
        .text_color(font_color)
        .build()
        .map_err(|e| anyhow!("Failed to build text layout - {e}"))
}

fn render_ring_icon(
    battery_level: u8,
    is_low_battery: bool,
    highlight_color: Option</* Hex color */ String>,
    background_color: Option</* Hex color */ String>,
    is_connect_color: Option<bool>,
) -> Result<(Vec<u8>, u32, u32)> {
    let width = 64;
    let height = 64;

    let mut device = Device::new().map_err(|e| anyhow!("Failed to get Device - {e}"))?;
    let mut bitmap_target = device
        .bitmap_target(width, height, 1.0)
        .map_err(|e| anyhow!("Failed to create a new bitmap target. - {e}"))?;
    let mut piet = bitmap_target.render_context();

    let center = (32.0, 32.0);
    let inner_radius = 20.0;
    let outer_radius = 30.0;
    let stroke_width = outer_radius - inner_radius;

    // 使用平均半径作为圆弧半径
    let arc_radius = (inner_radius + outer_radius) / 2.0;

    // 将电量转换为百分比并计算对应的角度
    let battery_angle = battery_level as f64 * 3.6;
    let battery_angle_rad = battery_angle.to_radians();

    // 定义圆环的样式（圆角端点）
    let style = StrokeStyle::new().line_cap(LineCap::Round);

    // 起始角度（顶部，-90°）
    let start_angle_rad = -std::f64::consts::PI / 2.0;

    // 间隙角度转换为弧度
    let gap_angle: f64 = if battery_level > 90 { 0.0 } else { 30.0 };
    let gap_angle_rad = gap_angle.to_radians();

    // 计算每个圆环应该缩短的角度（各分摊一半的间隙）
    let shorten_angle_rad = gap_angle_rad / 2.0;
    let not_custome_color = || {
        let is_connect = is_connect_color.unwrap_or(true); // None 视为 默认连接
        if is_connect {
            match SystemTheme::get() {
                SystemTheme::Light => Color::from_rgba32_u32(0x919191FF),
                SystemTheme::Dark => Color::from_rgba32_u32(0xDADADAFF),
            }
        } else {
            match SystemTheme::get() {
                SystemTheme::Light => Color::from_rgba32_u32(0xC4C4C4FF),
                SystemTheme::Dark => Color::from_rgba32_u32(0xDADADAA0),
            }
        }
    };
    // 绘制背景圆环（表示剩余电量）
    let background_sweep_angle =
        2.0 * std::f64::consts::PI - battery_angle_rad - 2.0 * shorten_angle_rad;
    let background_color = background_color
        .and_then(|hex| Color::from_hex_str(&hex).ok()) // 优先配置颜色
        .unwrap_or_else(not_custome_color);
    let background_arc = piet_common::kurbo::Arc {
        center: center.into(),
        radii: piet_common::kurbo::Vec2::new(arc_radius, arc_radius),
        start_angle: start_angle_rad + battery_angle_rad + shorten_angle_rad,
        sweep_angle: background_sweep_angle,
        x_rotation: 0.0,
    };
    piet.stroke_styled(background_arc, &background_color, stroke_width, &style);

    // 绘制高亮圆环（表示当前电量）
    let highlight_color = if is_low_battery {
        // 低电量颜色（不支持配置中自定义）
        is_connect_color
            .and_then(|is_connect| {
                is_connect
                    .then_some(Color::from_rgba32_u32(0xFE6666FF))
                    .or(Some(Color::from_rgba32_u32(0xFE6666C0)))
            })
            .unwrap_or(Color::from_rgba32_u32(0xFE6666FF))
    } else {
        highlight_color
            .and_then(|hex| Color::from_hex_str(&hex).ok()) // 优先配置颜色
            .unwrap_or_else(|| {
                is_connect_color
                    .and_then(|is_connect| {
                        is_connect
                            .then_some(Color::from_rgba32_u32(0x4CD083FF))
                            .or(Some(Color::from_rgba32_u32(0x4CD083A0)))
                    })
                    .unwrap_or(Color::from_rgba32_u32(0x4CD083FF))
            })
    };
    let highlight_arc = piet_common::kurbo::Arc {
        center: center.into(),
        radii: piet_common::kurbo::Vec2::new(arc_radius, arc_radius),
        start_angle: start_angle_rad + shorten_angle_rad,
        sweep_angle: battery_angle_rad - 2.0 * shorten_angle_rad,
        x_rotation: 0.0,
    };
    piet.stroke_styled(highlight_arc, &highlight_color, stroke_width, &style);

    piet.finish().map_err(|e| anyhow!("{e}"))?;
    drop(piet);

    let image_buf = bitmap_target.to_image_buf(ImageFormat::RgbaPremul).unwrap();

    Ok((
        image_buf.raw_pixels().to_vec(),
        image_buf.width() as u32,
        image_buf.height() as u32,
    ))
}
