use cargo_expunch::expuncher::Expuncher;
use cargo_metadata::MetadataCommand;
use std::env;
use std::path::PathBuf;

fn help() {
    println!(
        "expunch
Expand `use`d modules or declared `mod`ule in a Rust source-code into that contents in the workspace library crate
Rustソースコード中のuse文とモジュール宣言をワークスペースのライブラリクレートに含まれるソースコードの内容に展開する

USAGE:
    cargo expunch <source_code_path>

    * Use this subcommand at the directory of your workspace
      このサブコマンドはワークスペースのディレクトリで使用する必要があります

OPTIONS:
    h, --help               Prints help information
                            ヘルプを表示する

ARGS:
    source_code_path        Path to a Rust source code
                            Rustソースコードへのパス
"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        // コマンドライン引数が指定されていない
        1 => {
            eprintln!(
                "Specify the path to a Rust source-code in the option `source_code_path`
引数 source_code_path にRustソースコードへのパスを指定してください"
            )
        }
        // ヘルプ表示の指定
        2 if &args[1] == "-h" || &args[1] == "--help" => {
            // ヘルプを表示
            help();
        }
        // コマンドライン引数を必要数指定した
        2 => {
            // 引数
            let source_code_path = &args[1];

            // 実行
            if let Err(message) = expunch_file(source_code_path) {
                eprintln!("{}", message);
            };
        }
        // 必要外の指定
        _ => {
            // ヘルプを表示
            help();
        }
    }
}

/// Rustソースコードを解析して展開する
fn expunch_file(source_code_path: &str) -> Result<(), String> {
    let source_code_path = PathBuf::from(source_code_path);
    let package_path = PathBuf::from(".");
    let metadata = MetadataCommand::new()
        .manifest_path("./Cargo.toml")
        .current_dir(&package_path)
        .exec()
        .unwrap();
    let package = metadata.root_package().unwrap();

    let mut expuncher = Expuncher::new(&package.name, package_path.join("src"));
    expuncher.analyze_source_file(&source_code_path)?;
    expuncher.dump()?;

    Ok(())
}
