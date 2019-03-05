use win_test::mswin;

#[macro_use]
extern crate log;
extern crate stderrlog;

fn main() {
    stderrlog::new()
        .module(module_path!())
        .verbosity(6)
        .timestamp(stderrlog::Timestamp::Millisecond)
        .init()
        .unwrap();

    trace!("wint_test started");
    if mswin::available() {
        mswin::process().unwrap()
    } else {
        println!("Error: not a supported operating system");
        std::process::exit(1);
    }
}
