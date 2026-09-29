# Project Instructions for AI Agents
- Keep every individual Rust source file (`*.rs`) at or below 600 lines. Split a file into modules before it exceeds this limit.
- After finishing code changes, run `./update-header` to add missing license headers, then run `./format-code` to format Rust code.
- After that, you'll have to run `./lint-code` to ensure you have passed the linting tests. Any changes that failed the linting tests WILL BE REJECTED.
- Before reporting completion, verify that no `*.rs` file exceeds 600 lines and that `./update-header --check` and `./format-code --check` pass.
- Your code should be as concise as possible—to ensure it remains human-readable and maintainable. You should include comments that clearly explain the intent of the code.
- Comments must be written in English; please use simple English so that non-native English speakers can understand them. If you need to update the documentation, do so in the respective language of each document.
- The goal of the code is to be human-readable and maintainable; use this as the foundation to complete your task.
- Please refer to `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md`; all requirements applicable to human contributors and AI policies apply to you as well.
