//! Vertical stacking of rendered PDF pages into a single PNG, ported from the
//! desktop `stack_pages_vertically` (white background, gray separators). Pure
//! bytes in, bytes out so page renders stay mockable (task 19).

use std::io::Cursor;

use image::{DynamicImage, Rgba, RgbaImage};
use thiserror::Error;

pub(crate) const PAGE_SEPARATOR_HEIGHT: u32 = 6;
const PAGE_SEPARATOR_GRAY: Rgba<u8> = Rgba([203, 203, 203, 255]);

#[derive(Debug, Error)]
pub enum PngStackError {
    #[error("Impossible de lire une page rendue du PDF.")]
    Decode(#[source] image::ImageError),
    #[error("Impossible d'encoder le PNG empilé.")]
    Encode(#[source] image::ImageError),
}

/// Stacks one PNG per page, in order, separated by a gray rule on white.
/// Expects at least one page (the caller guards the empty case).
pub fn stack_pages_vertically(page_pngs: &[Vec<u8>]) -> Result<Vec<u8>, PngStackError> {
    let mut pages = Vec::with_capacity(page_pngs.len());
    for png in page_pngs {
        pages.push(
            image::load_from_memory(png)
                .map_err(PngStackError::Decode)?
                .into_rgba8(),
        );
    }

    let width = pages.iter().map(RgbaImage::width).max().unwrap_or(1);
    let separators = (pages.len() as u32).saturating_sub(1) * PAGE_SEPARATOR_HEIGHT;
    let height = pages.iter().map(RgbaImage::height).sum::<u32>() + separators;

    let mut stacked = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let mut cursor = 0_u32;
    for (index, page) in pages.iter().enumerate() {
        if index > 0 {
            for y in cursor..cursor + PAGE_SEPARATOR_HEIGHT {
                for x in 0..width {
                    stacked.put_pixel(x, y, PAGE_SEPARATOR_GRAY);
                }
            }
            cursor += PAGE_SEPARATOR_HEIGHT;
        }
        image::imageops::overlay(&mut stacked, page, 0, i64::from(cursor));
        cursor += page.height();
    }

    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(stacked)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(PngStackError::Encode)?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, Rgba, RgbaImage};

    use super::{PAGE_SEPARATOR_GRAY, PAGE_SEPARATOR_HEIGHT, stack_pages_vertically};

    fn png_of(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba(color)));
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encode test page");
        bytes.into_inner()
    }

    fn decode(png: &[u8]) -> RgbaImage {
        image::load_from_memory(png)
            .expect("decode stacked png")
            .into_rgba8()
    }

    #[test]
    fn stacks_pages_in_order_with_a_gray_separator_on_white() {
        let first = png_of(10, 4, [10, 20, 30, 255]);
        let second = png_of(8, 6, [40, 50, 60, 255]);

        let stacked =
            decode(&stack_pages_vertically(&[first, second]).expect("stacking should succeed"));

        assert_eq!(stacked.width(), 10);
        assert_eq!(stacked.height(), 4 + PAGE_SEPARATOR_HEIGHT + 6);
        assert_eq!(stacked.get_pixel(0, 0), &Rgba([10, 20, 30, 255]));
        assert_eq!(stacked.get_pixel(0, 4), &PAGE_SEPARATOR_GRAY);
        assert_eq!(
            stacked.get_pixel(0, 4 + PAGE_SEPARATOR_HEIGHT),
            &Rgba([40, 50, 60, 255])
        );
        // The narrower second page leaves the canvas white on its right.
        assert_eq!(
            stacked.get_pixel(9, 4 + PAGE_SEPARATOR_HEIGHT),
            &Rgba([255, 255, 255, 255])
        );
    }

    #[test]
    fn keeps_three_pages_in_order() {
        let pages: Vec<Vec<u8>> = [1_u8, 2, 3]
            .iter()
            .map(|index| png_of(2, 2, [*index, 0, 0, 255]))
            .collect();

        let stacked = decode(&stack_pages_vertically(&pages).expect("stacking should succeed"));

        let stride = 2 + PAGE_SEPARATOR_HEIGHT;
        assert_eq!(stacked.get_pixel(0, 0), &Rgba([1, 0, 0, 255]));
        assert_eq!(stacked.get_pixel(0, stride), &Rgba([2, 0, 0, 255]));
        assert_eq!(stacked.get_pixel(0, 2 * stride), &Rgba([3, 0, 0, 255]));
    }

    #[test]
    fn a_single_page_stacks_without_separator() {
        let only = png_of(10, 4, [10, 20, 30, 255]);

        let stacked = decode(&stack_pages_vertically(&[only]).expect("stacking should succeed"));

        assert_eq!(stacked.width(), 10);
        assert_eq!(stacked.height(), 4);
        assert_eq!(stacked.get_pixel(0, 3), &Rgba([10, 20, 30, 255]));
    }

    #[test]
    fn rejects_undecodable_page_bytes() {
        let broken = vec![1_u8, 2, 3, 4];

        let result = stack_pages_vertically(&[broken]);

        assert!(result.is_err());
    }
}
