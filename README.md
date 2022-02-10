# cargo-expunch

Cargo subcommand to expand `use`d modules or declared `mod`ule in a Rust source-code into that contents in the workspace library crate.

Rustソースコード中のuse文とモジュール宣言をワークスペースのライブラリクレートに含まれるソースコードの内容に展開するCargoのサブコマンドです。

## Installation
```sh
cargo install --git https://github.com/accup/cargo-expunch.git
```

## Usage
```sh
cargo expunch <source_code_path>
```

* Use this subcommand at the directory of your workspace

    このサブコマンドはワークスペースのディレクトリで使用する必要があります

### Example
#### File contents
##### `Cargo.toml`
```toml
[package]
name = "example"
# ...
```

##### `src/main.rs`
```rs
use example::{self, foo};

fn main() {
    println!("Hello, world!");
}
```

##### `src/lib.rs`
```rs
pub mod foo;

pub fn good_afternoon() {}
```

##### `src/foo/mod.rs`
```rs
mod bar;

pub fn good_evening() {}
```

##### `src/foo/bar.rs`
```rs
pub fn good_morning() {}
```

#### Output

Use of the `example` module is removed and the contents of the library crate are appended.

`example`モジュールのuseが削除され、ライブラリクレートの内容が末尾に展開されます。

##### Standard output of the command `cargo expunch ./src/main.rs`
```rs
use example :: { foo } ;

fn main() {
    println!("Hello, world!");
}

mod example {


pub fn good_afternoon() {}

pub mod foo {


pub fn good_evening() {}

mod bar {
pub fn good_morning() {}
}
}
}
```

##### Standard output of the command `cargo expunch ./src/main.rs | rustfmt`
```rs
use example::foo;

fn main() {
    println!("Hello, world!");
}

mod example {

    pub fn good_afternoon() {}

    pub mod foo {

        pub fn good_evening() {}

        mod bar {
            pub fn good_morning() {}
        }
    }
}
```
