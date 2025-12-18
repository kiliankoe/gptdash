See @README.md for project overview and game documentation, and @ARCHITECTURE.md for technical architecture details, make sure to update those files as necessary when making changes to this project.

Where sensible, add fitting tests (unit and integration tests) to verify functionality, but keep tests minimal and focused on critical logic. And if possible, use a TDD approach, adding failing tests before implementing changes. There is a set of end-to-end playwright tests testing the entire game flow in `e2e/`. Please keep those tests up to date as necessary when making changes to the game flow and run them to verify everything works as expected. The output of these tests can be quite long, please only tail their logs. Test suites can be run with `cargo test` and `npm run test:e2e`.

Be sure to always run `cargo fmt`, `cargo check`, `cargo clippy`, `biome lint`, and `biome format --write` to ensure code style and quality. There is no npm project, biome is installed globally.
