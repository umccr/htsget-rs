# Contributing to htsget-rs

Thank you for your interest in contributing. We greatly value feedback and contributions, whether that's
an issue, bug fix, new feature or document change. All contributions are welcome, and no change is too small, even if
its just a typo fix.

To get familiar with the project, have a look at the READMEs of each crate.

## Issues

You are welcome to use [issues] to submit a bug report or new feature request. Issues are used to open up discussion for
suggested changes or address bugs.

Have a look at existing [issues] to see if an issue has already been discussed.

[issues]: https://github.com/umccr/htsget-rs/issues

## Pull Requests
 
We welcome you to open up a pull request 
to suggest a change, even if it's a small one line change. If the change is large, it is a good idea to first open an 
issue to discuss the change in order gain feedback and guidance.

### Tests and formatting

If the proposed change alters the code, tests should updated to ensure that no regressions are made. Any new features 
need to have thorough testing before they are merged. 

We also use [clippy] and [rustfmt] for code style, linting and formatting.

Please run the following commands to check tests, lints, code style and formatting before submitting a pull request:

```sh
cargo clippy --all-targets --all-features
cargo fmt
cargo test --tests --all-features
```

[clippy]: https://github.com/rust-lang/rust-clippy
[rustfmt]: https://github.com/rust-lang/rustfmt

## Code of conduct

We follow the [Rust Code of conduct][rust-code-of-conduct]. For moderation, please contact the maintainers of this
project directly, at mmalenic1@gmail.com ([@mmalenic]).

[rust-code-of-conduct]: https://www.rust-lang.org/policies/code-of-conduct
[@mmalenic]: https://github.com/mmalenic

## License

This project is licensed under the [MIT license][license]. Unless otherwise stated, any contribution submitted 
by you will also be licensed under the [MIT license][license].

[license]: LICENSE