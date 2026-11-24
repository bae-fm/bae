use super::*;

#[test]
fn levels_halve_from_the_picture_s_size_down_to_one_tile() {
    let pyramid = Pyramid { width: 6000, height: 5600, format: CopyFormat::Jpeg };
    let sizes: Vec<_> = pyramid.levels().iter().map(|level| (level.width, level.height)).collect();
    assert_eq!(sizes, [(6000, 5600), (3000, 2800), (1500, 1400), (750, 700), (375, 350)]);
    let first = pyramid.levels()[0];
    assert_eq!((first.columns, first.rows), (12, 11));
}

#[test]
fn a_picture_that_fits_one_tile_has_one_level() {
    let pyramid = Pyramid { width: 512, height: 300, format: CopyFormat::Png };
    assert_eq!(pyramid.levels(), [TileLevel { width: 512, height: 300, columns: 1, rows: 1 }]);
}

/// A stored area, as its left, top, width and height.
fn area(left: u32, top: u32, width: u32, height: u32) -> Area {
    Area { left, top, width, height }
}

/// For a stored picture 1100 by 700, each EXIF flag's stored area under the
/// upright picture's first tile and last tile, worked out from where the flag
/// puts the picture's corners: the first tile is 512 by 512 at the upright
/// picture's top left, the last the remainder at its bottom right.
#[test]
fn each_upright_tile_comes_from_the_stored_area_that_holds_its_pixels() {
    let first = area(0, 0, 512, 512);
    let upright_last = area(1024, 512, 76, 188);
    let turned_last = area(512, 1024, 188, 76);
    // (flag, the last upright tile, the stored area of the first, of the last)
    let cases = [
        (1, upright_last, area(0, 0, 512, 512), area(1024, 512, 76, 188)),
        (2, upright_last, area(588, 0, 512, 512), area(0, 512, 76, 188)),
        (3, upright_last, area(588, 188, 512, 512), area(0, 0, 76, 188)),
        (4, upright_last, area(0, 188, 512, 512), area(1024, 0, 76, 188)),
        (5, turned_last, area(0, 0, 512, 512), area(1024, 512, 76, 188)),
        (6, turned_last, area(0, 188, 512, 512), area(1024, 0, 76, 188)),
        (7, turned_last, area(588, 188, 512, 512), area(0, 0, 76, 188)),
        (8, turned_last, area(588, 0, 512, 512), area(0, 512, 76, 188)),
    ];
    for (flag, last, stored_first, stored_last) in cases {
        let orientation = Orientation::of_flag(flag);
        assert_eq!(orientation.stored_area(1100, 700, first), stored_first, "flag {flag}, first");
        assert_eq!(orientation.stored_area(1100, 700, last), stored_last, "flag {flag}, last");
    }
}

#[test]
fn a_tile_s_file_is_in_its_level_s_folder() {
    let folder = Path::new("/tiles");
    assert_eq!(tile_path(folder, CopyFormat::Jpeg, 2, 3, 4), Path::new("/tiles/2/3_4.jpg"));
}
