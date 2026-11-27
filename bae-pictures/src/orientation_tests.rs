use super::*;

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
