use duplicate_destroyer;

fn main() {
    env_logger::init();

    duplicate_destroyer::get_duplicates(vec!["./".into()]);
}
