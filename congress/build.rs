fn main() {
    println!("cargo:rerun-if-changed=static/js/");
    println!("cargo:rerun-if-changed=scripts/build-frontend.mjs");
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=package-lock.json");

    let status = std::process::Command::new("npm")
        .args(["ci", "--ignore-scripts"])
        .status()
        .expect("Failed to run npm ci - is npm installed?");
    assert!(status.success(), "npm ci failed");

    let status = std::process::Command::new("npm")
        .args(["run", "build:frontend"])
        .status()
        .expect("Failed to run frontend build");
    assert!(status.success(), "Frontend build failed");
}
