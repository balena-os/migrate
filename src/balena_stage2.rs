use balena_migrate::stage2;

fn main() {
    if let Err(error) = stage2() {
        println!("got error from stage2: {}", error);
    }
}
