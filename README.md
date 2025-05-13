<!-- cargo-rdme start -->

# `lkml`

A command line tool to download mailing list emails via `lei` and then assort them into
maildirs based on custom criteria.

<div class="warning">

**WARNING**: this program is still pretty experimental and might break your email, use with
care!

</div>


## Configuration

See [`Config`](https://docs.rs/lkml/latest/lkml/config/struct.Config.html) for the various configuration options. The location of
the config file is `~/.config/lkml/config.toml` on linux.

<!-- cargo-rdme end -->
