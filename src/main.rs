use win_test::mswin;

fn main() {
    if mswin::available() {
        mswin::process().unwrap()
    } else {
        println!("Error: not a supported operating system");
        std::process::exit(1);
    }
}
