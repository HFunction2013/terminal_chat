# sl-rs

Pure Rust port of the SL, an old joke command.

SL (Steam Locomotive) runs across your terminal when you type "sl" as you meant to type "ls".

The [original version](https://github.com/mtoyoda/sl) was written in C by Toyoda Masashi (mtoyoda@acm.org).
The original license is available in `LICENSE-ORIGINAL`.

*Japanese follows English*

## Install

```shell
% cargo build --release
% sudo install -s target/release/sl /usr/local/bin
```

or 

```shell
% cargo install sl-rs
```

## Usage

```shell
% sl --help
% sl
% sl -laF
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.


## 概要
長い歴史をもつジョークソフト SL をRustに移植したものです。

