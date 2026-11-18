use std::path::PathBuf;
use std::{env, fs};

/// With `EMBEDDED_DUMP=<dir>` set, writes the frame as `<dir>/<name>.ppm`,
/// where `rgb(x, y)` is the colour of a pixel.
pub fn dump(name: &str, width: u32, height: u32, rgb: impl Fn(u32, u32) -> [u8; 3]) {
    let Some(dir) = env::var_os("EMBEDDED_DUMP") else {
        return;
    };
    let dir = PathBuf::from(dir);
    fs::create_dir_all(&dir).unwrap();
    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    for y in 0..height {
        for x in 0..width {
            data.extend(rgb(x, y));
        }
    }
    fs::write(dir.join(format!("{name}.ppm")), data).unwrap();
}
