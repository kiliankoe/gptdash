fn main() {
    println!("cargo:rerun-if-changed=static/js/");
    println!("cargo:rerun-if-changed=scripts/build-frontend.mjs");

    let status = std::process::Command::new("npm")
        .args(["run", "build:frontend"])
        .status()
        .expect("Failed to run frontend build - is npm installed?");

    assert!(status.success(), "Frontend build failed");
}
