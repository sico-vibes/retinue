// The GUI subsystem only exists on Windows; on Linux the binary is an ordinary
// executable that GTK starts from the desktop entry.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

fn main() {
    retinue_desktop::run();
}
