use std::process::Command;

fn main() {
    Command::new("git").arg("diff").status().expect("失敗");
}
