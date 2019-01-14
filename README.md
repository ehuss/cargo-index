# `cargo index`

[![crates.io](https://img.shields.io/crates/v/cargo-index.svg)](https://crates.io/crates/cargo-index)

An experimental [Cargo] subcommand to access and manipulate a registry index.

See [reg-index] for the corresponding library that implements this command's
functionality.

[Cargo]: https://doc.rust-lang.org/cargo/
[reg-index]: https://github.com/ehuss/cargo-index/tree/master/reg-index

## Installation

`cargo install cargo-index`

This requires at a minimum Cargo 1.33.

## Usage

The `cargo index` command provides several sub-commands:

Subcommand | Description
---------- | -----------
add        | Add a package to an index.
init       | Create a new index.
list       | List entries in the index.
metadata   | Generate JSON metadata for a package.
unyank     | Un-yank a crate from an index.
validate   | Validate the format of an index.
yank       | Yank a crate from an index.

Run the sub-command with `--help` to get more information.

### Example

Example of creating an index and manually adding a new package:

1. `cargo index init --dl https://example.com --index index`

    This creates a new git repository in the directory `index` with the
    appropriate `config.json` file.

2. `cargo new foo`

    Create a sample project to add.

3. `cd foo`

4. `cargo index add  --index ../index --index-url https://example.com -- --allow-dirty`

    Adds the `foo` package to the index.

5. `cargo index list --index ../index -p foo`

    Shows the JSON metadata for every version of `foo` in the index.
