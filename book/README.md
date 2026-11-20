# The Road to Quokka

The book, written alongside the system it describes.

Every file here is plain Markdown and reads fine in a text editor or on GitHub.
`book.toml` additionally lets [mdBook](https://rust-lang.github.io/mdBook/) build
it as a website:

```
cargo install mdbook        # once
mdbook serve book --open    # live preview
mdbook build book           # HTML into book/book/
```

mdBook is **not** required to build Quokka or to contribute to the book. It is
not in CI and not a dependency of anything. `scripts/check.sh` only verifies that
every chapter listed in `src/SUMMARY.md` exists.

See [issue #8](https://github.com/quokka-lisp/quokka-lisp/issues/8) for what is
written and what is planned.
