use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg::{Options, Tree},
};

use super::models::CaptchaPoint;

const SVG_PREFIX: &str = "data:image/svg+xml;base64,";
const CAPTCHA_ASPECT_WIDTH: u32 = 360;
const CAPTCHA_ASPECT_HEIGHT: u32 = 236;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptchaRaster {
    width: u32,
    height: u32,
    active: Vec<bool>,
}

impl CaptchaRaster {
    pub fn from_data_url(value: &str) -> Result<Self> {
        let encoded = value
            .trim()
            .strip_prefix(SVG_PREFIX)
            .context("сервер прислал CAPTCHA не в SVG")?;
        let svg = STANDARD
            .decode(encoded)
            .context("сервер прислал повреждённую CAPTCHA")?;
        let tree = Tree::from_data(&svg, &Options::default())
            .context("не удалось разобрать SVG CAPTCHA")?;
        let size = tree.size().to_int_size();
        if size.width() == 0 || size.height() == 0 || size.width() > 2_048 || size.height() > 2_048
        {
            bail!("размер CAPTCHA выглядит подозрительно")
        }
        let mut pixmap = Pixmap::new(size.width(), size.height())
            .context("не удалось выделить память для CAPTCHA")?;
        resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
        let active = pixmap
            .pixels()
            .iter()
            .map(|pixel| {
                let red = pixel.red();
                let green = pixel.green();
                let blue = pixel.blue();
                let brightest = red.max(green).max(blue);
                let darkest = red.min(green).min(blue);
                brightest > 72 || brightest.saturating_sub(darkest) > 24
            })
            .collect();
        Ok(Self {
            width: size.width(),
            height: size.height(),
            active,
        })
    }

    pub fn lines(&self, columns: u16, rows: u16) -> Vec<String> {
        (0..rows)
            .map(|row| {
                (0..columns)
                    .map(|column| {
                        let top = self.region_active(column, row * 2, columns, rows * 2);
                        let bottom = self.region_active(column, row * 2 + 1, columns, rows * 2);
                        match (top, bottom) {
                            (false, false) => ' ',
                            (true, false) => '▀',
                            (false, true) => '▄',
                            (true, true) => '█',
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn region_active(&self, x: u16, y: u16, columns: u16, pixel_rows: u16) -> bool {
        if columns == 0 || pixel_rows == 0 {
            return false;
        }
        let x0 = u32::from(x) * self.width / u32::from(columns);
        let x1 = (u32::from(x + 1) * self.width / u32::from(columns)).max(x0 + 1);
        let y0 = u32::from(y) * self.height / u32::from(pixel_rows);
        let y1 = (u32::from(y + 1) * self.height / u32::from(pixel_rows)).max(y0 + 1);
        let mut lit = 0_u32;
        let mut total = 0_u32;
        for row in y0..y1.min(self.height) {
            for column in x0..x1.min(self.width) {
                total += 1;
                if self.active[(row * self.width + column) as usize] {
                    lit += 1;
                }
            }
        }
        total > 0 && lit * 20 >= total
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellArea {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

pub fn account_popup_area(terminal_width: u16, terminal_height: u16) -> CellArea {
    let width = terminal_width.saturating_sub(4).min(82);
    let height = terminal_height.saturating_sub(2).min(31);
    CellArea {
        x: terminal_width.saturating_sub(width) / 2,
        y: terminal_height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub fn captcha_cell_area(terminal_width: u16, terminal_height: u16) -> CellArea {
    let popup = account_popup_area(terminal_width, terminal_height);
    let width = popup.width.saturating_sub(4);
    let ideal_height =
        ((u32::from(width) * CAPTCHA_ASPECT_HEIGHT / CAPTCHA_ASPECT_WIDTH) / 2).max(1) as u16;
    let height = ideal_height.min(popup.height.saturating_sub(7));
    CellArea {
        x: popup.x.saturating_add(2),
        y: popup.y.saturating_add(3),
        width,
        height,
    }
}

pub fn click_to_point(area: CellArea, column: u16, row: u16) -> Option<CaptchaPoint> {
    if column < area.x
        || row < area.y
        || column >= area.x.saturating_add(area.width)
        || row >= area.y.saturating_add(area.height)
        || area.width == 0
        || area.height == 0
    {
        return None;
    }
    Some(CaptchaPoint {
        x: (f64::from(column - area.x) + 0.5) / f64::from(area.width),
        y: (f64::from(row - area.y) + 0.5) / f64::from(area.height),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_svg_becomes_terminal_blocks() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#050607"/><rect x="5" y="5" width="10" height="10" fill="#ffffff"/></svg>"##;
        let data_url = format!("{SVG_PREFIX}{}", STANDARD.encode(svg));

        let raster = CaptchaRaster::from_data_url(&data_url).unwrap();
        let lines = raster.lines(10, 5);

        assert_eq!(lines.len(), 5);
        assert!(lines.iter().any(|line| line.contains('█')));
    }

    #[test]
    fn click_mapping_rejects_border_and_normalizes_inside() {
        let area = CellArea {
            x: 10,
            y: 5,
            width: 40,
            height: 20,
        };

        assert!(click_to_point(area, 9, 10).is_none());
        assert_eq!(
            click_to_point(area, 10, 5),
            Some(CaptchaPoint {
                x: 0.0125,
                y: 0.025,
            })
        );
    }

    #[test]
    fn captcha_layout_fits_the_terminal() {
        let area = captcha_cell_area(100, 32);
        assert!(area.x + area.width <= 100);
        assert!(area.y + area.height <= 32);
        assert!(area.height >= 20);
    }
}
