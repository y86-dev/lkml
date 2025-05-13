[![Crates.io](https://img.shields.io/crates/v/lkml.svg)](https://crates.io/crates/lkml)
[![Documentation](https://docs.rs/lkml/badge.svg)](https://docs.rs/lkml/)
[![Dependency status](https://deps.rs/repo/github/y86-dev/lkml/status.svg)](https://deps.rs/repo/github/y86-dev/lkml)
![License](https://img.shields.io/crates/l/lkml)
![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/y86-dev/lkml/ci.yml)

<!-- cargo-rdme start -->

# `lkml`

A command line tool to download mailing list emails via `lei` and then assort them into
maildirs based on custom criteria.

<div class="warning">

**WARNING**: this program is still pretty experimental and might break your email, use with
care!

</div>


## Configuration

See [`Config`] for the various configuration options. The location of the config file is
`~/.config/lkml/config.toml` on linux.

<!-- cargo-rdme end -->

[`Config`]: https://docs.rs/lkml/latest/lkml/config/struct.Config.html
