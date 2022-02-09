use crate::module::*;
use proc_macro2::LineColumn;
use quote::ToTokens;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, prelude::*, BufReader, Read};
use std::path::PathBuf;
use syn::{self, spanned::Spanned, Ident, Item, ItemUse, UseGroup, UseName, UsePath, UseTree};

#[derive(Debug)]
pub struct Expuncher {
    collected_modules: ModuleNode,
    package_name: String,
    package_src_path: PathBuf,
    crate_visibility: String,
}

impl Expuncher {
    /// 新たなエクスパンチャを作成する
    ///
    /// # Arguments
    ///
    /// * `package_name` モジュールの解決に用いるパッケージの名前
    ///
    /// * `package_src_path` パッケージのsrcディレクトリへのパス
    ///
    /// * `crate_path` クレートのパス
    pub fn new(package_name: &str, package_src_path: PathBuf) -> Expuncher {
        Expuncher {
            collected_modules: ModuleNode::new(),
            package_name: String::from(package_name),
            package_src_path,
            crate_visibility: String::from("pub "),
        }
    }

    /// ファイルの内容を基にすべての依存するモジュールを解析する
    ///
    /// # Arguments
    ///
    /// * `source_path` ソースコードへのパス
    pub fn analyze_source_file(&mut self, source_path: &PathBuf) -> Result<(), String> {
        let source_parts = Vec::new();
        self.collected_modules
            .update(&source_parts, source_path.clone());

        self.analyze_file_impl(source_path, &source_parts, "crate", source_path)?;
        self.collected_modules.sort_replacement_spans();
        Ok(())
    }

    /// ファイルの内容を基にすべての依存するモジュールを解析する
    ///
    /// # Arguments
    ///
    /// * `source_path` ソースコードへのパス
    ///
    /// * `parts_prefix` モジュール解決のためのモジュールパスの接頭辞
    pub fn analyze_file(
        &mut self,
        source_path: &PathBuf,
        source_parts: &[String],
    ) -> Result<(), String> {
        self.analyze_file_impl(source_path, source_parts, "crate", source_path)?;
        self.collected_modules.sort_replacement_spans();
        Ok(())
    }

    fn analyze_file_impl(
        &mut self,
        source_path: &PathBuf,
        source_parts: &[String],
        crate_name: &str,
        crate_path: &PathBuf,
    ) -> Result<(), String> {
        let mut file = File::open(source_path).or_else(|_| {
            Err(format!(
                "File {0} not exists
ファイル {1} が存在しません",
                source_path.to_str().unwrap_or("(undisplayable path)"),
                source_path.to_str().unwrap_or("（表示できないパス）"),
            )
            .to_owned())
        })?;
        let mut content = String::new();
        file.read_to_string(&mut content).or_else(|_| {
            Err(format!(
                "Failed to read the file {0}
ファイル {1} の読み取りに失敗しました",
                source_path.to_str().unwrap_or("(undisplayable path)"),
                source_path.to_str().unwrap_or("（表示できないパス）"),
            ))
        })?;
        let ast = syn::parse_file(&content).or_else(|_| {
            Err(format!(
                "Failed to parse the source-code {0}
ソースコード {1} の構文解析に失敗しました",
                source_path.to_str().unwrap_or("(undisplayable path)"),
                source_path.to_str().unwrap_or("（表示できないパス）"),
            ))
        })?;

        // selfパスの解決
        let self_path = match source_path.file_name() {
            Some(name) if name == "mod.rs" => source_path
                .parent()
                .ok_or_else(|| {
                    format!(
                        "Failed to get the parent directory of the {0}
{1} より上の階層へ遡ろうとしました",
                        source_path.to_str().unwrap_or("(undisplayable path)"),
                        source_path.to_str().unwrap_or("（表示できないパス）"),
                    )
                })?
                .to_path_buf(),
            _ => source_path.clone(),
        };

        for item in &ast.items {
            // トップレベルのuse文を解析
            if let Item::Use(item_use) = item {
                // use文から依存モジュールを取得
                let module_items = collect_module_items(
                    &item_use.tree,
                    &self.package_name,
                    &self.package_src_path,
                    crate_name,
                    crate_path,
                    &self_path,
                )?;

                for module_item in &module_items {
                    // useの途中に現れるモジュールも含めて解決
                    let (ModuleItemAccessibility::Indirect(module_item_path)
                    | ModuleItemAccessibility::Direct(module_item_path)) = module_item;

                    // モジュールのパス
                    let (ModuleItemPath::File(parts, _)
                    | ModuleItemPath::Dir(parts, _)
                    | ModuleItemPath::Insoluble(parts)) = module_item_path;
                    // モジュールパスの結合
                    let full_parts = concat_module_parts(source_parts, parts, crate_name);
                    // モジュールの参照先がライブラリクレートか
                    let is_lib_crate = &full_parts == &[self.package_name.clone()];

                    // ファイルが解決されるモジュールのみを登録
                    if let ModuleItemPath::File(_, path) = module_item_path {
                        // ソースコードが依存するモジュールを登録
                        if let None = self.collected_modules.update(&full_parts, path.clone()) {
                            // 依存するモジュールのソースコードを解析
                            self.analyze_file_impl(
                                path,
                                &full_parts,
                                // ライブラリクレートの場合はクレートを変更する
                                &String::from(if is_lib_crate {
                                    &self.package_name
                                } else {
                                    crate_name
                                }),
                                if is_lib_crate { path } else { crate_path },
                            )?;
                        }
                    }

                    // トップレベルのソースコードの解析時でありライブラリクレートが直接useされている場合に限り
                    // クレートの可視性をuseの指定に合わせる
                    if is_lib_crate && source_parts.is_empty() {
                        self.crate_visibility = item_use.vis.to_token_stream().to_string() + " ";
                    }
                }

                // `crate`の解決
                let use_tree = self.resolve_modules(&item_use.tree, crate_name);

                let use_tree = if source_parts.is_empty() {
                    // トップレベルのソースコードの解析時に限りトップレベルのモジュールのuseを削除する
                    self.remove_top_module(&use_tree, crate_name)
                } else {
                    Some(use_tree)
                };

                // use文の削除置換の追加
                if let Some(replacement_spans) =
                    self.collected_modules.replacement_spans_mut(&source_parts)
                {
                    let span = item.span();
                    replacement_spans.push(ReplacementSpan {
                        start: span.start(),
                        end: span.end(),
                        replacement: if let Some(use_tree) = use_tree {
                            Item::Use(ItemUse {
                                attrs: item_use.attrs.clone(),
                                vis: item_use.vis.clone(),
                                use_token: item_use.use_token,
                                leading_colon: item_use.leading_colon,
                                tree: use_tree,
                                semi_token: item_use.semi_token,
                            })
                            .to_token_stream()
                            .to_string()
                        } else {
                            String::new()
                        },
                    });
                }
            }
            // トップレベルのmod文を解析
            else if let Item::Mod(item_mod) = item {
                // 宣言文の場合のみ処理
                if let None = item_mod.content {
                    // モジュールパスの結合
                    let full_parts = concat_module_parts(
                        source_parts,
                        &vec![item_mod.ident.to_string()],
                        crate_name,
                    );

                    // mod文から依存モジュールを取得
                    let module_item_path = make_module_item_path(
                        &full_parts,
                        &self.package_name,
                        &self.package_src_path,
                        crate_path,
                        &self_path,
                    )?;

                    // ファイルが解決されるモジュールのみを登録
                    if let ModuleItemPath::File(_, path) = &module_item_path {
                        // mod文の削除置換
                        if let Some(replacement_spans) =
                            self.collected_modules.replacement_spans_mut(&source_parts)
                        {
                            let span = item.span();
                            replacement_spans.push(ReplacementSpan {
                                start: span.start(),
                                end: span.end(),
                                replacement: String::new(),
                            });
                        }

                        // ソースコードが依存するモジュールを登録
                        if let None = self.collected_modules.update(&full_parts, path.clone()) {
                            // 新たに登録できた場合にのみ依存するモジュールのソースコードを解析
                            // 注：mod宣言ではクレートは変更されない
                            self.analyze_file_impl(path, &full_parts, crate_name, crate_path)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 解析した内容に基づいてソースコードを標準出力に出力する
    pub fn dump(&self) -> Result<(), String> {
        self.dump_module(&self.collected_modules, &Vec::new(), "crate")
    }

    fn dump_module(
        &self,
        module: &ModuleNode,
        source_parts: &[String],
        crate_name: &str,
    ) -> Result<(), String> {
        if let Some(source_path) = &module.path {
            let file = File::open(source_path).or_else(|_| {
                Err(format!(
                    "File {0} not exists
                    ファイル {1} が存在しません",
                    source_path.to_str().unwrap_or("(undisplayable path)"),
                    source_path.to_str().unwrap_or("（表示できないパス）"),
                )
                .to_owned())
            })?;

            // 既に置換の配列はソート済みとする
            let mut replacement_spans_iter = module.replacement_spans.iter();
            let mut replacement_span_or_none = replacement_spans_iter.next();

            // 注：LineColumn::columnはUTF-8文字としてのカウントである
            for (line_number, line) in BufReader::new(file).lines().enumerate() {
                let line_number = line_number + 1; // 1-indexed

                if let io::Result::Ok(line) = line {
                    if let Some(replacement_span) = replacement_span_or_none {
                        if line_number < replacement_span.start.line {
                            // 置換開始行以前はそのままの一行を出力
                            print!("{}", line);
                        } else if line_number == replacement_span.start.line {
                            // 置換開始行は置換開始列以前の文字列を出力
                            let pre_line: String =
                                line.chars().take(replacement_span.start.column).collect();
                            print!("{}", pre_line);
                            // 置換文字列を出力
                            print!("{}", replacement_span.replacement);
                        }

                        if line_number == replacement_span.end.line {
                            // 置換終了行は置換終了列以降の文字列を出力
                            let post_line: String =
                                line.chars().skip(replacement_span.end.column).collect();
                            print!("{}", post_line);

                            // 次の置換に遷移
                            replacement_span_or_none = replacement_spans_iter.next();
                        }
                    } else {
                        // 置換が存在しない場合はそのまま出力
                        print!("{}", line);
                    }

                    // 改行
                    println!();
                }
            }
        }

        // 依存するソースコードを展開
        for (name, child) in &module.children {
            // モジュールパスの結合
            let full_parts = concat_module_parts(source_parts, &vec![name.clone()], crate_name);
            // モジュールの参照先がライブラリクレートか
            let is_lib_crate = &full_parts == &[self.package_name.clone()];

            println!();
            println!(
                "{}mod {} {{",
                if source_parts.is_empty() && name == &self.package_name {
                    &self.crate_visibility
                } else {
                    "pub "
                },
                name
            );
            self.dump_module(
                child,
                &full_parts,
                // ライブラリクレートの場合はクレートを変更する
                &String::from(if is_lib_crate {
                    &self.package_name
                } else {
                    crate_name
                }),
            )?;
            println!("}}");
        }

        Ok(())
    }

    pub fn remove_top_module(&self, use_tree: &UseTree, crate_name: &str) -> Option<UseTree> {
        self.remove_top_module_impl(use_tree, crate_name, 0)
    }

    fn remove_top_module_impl(
        &self,
        use_tree: &UseTree,
        crate_name: &str,
        depth: usize,
    ) -> Option<UseTree> {
        match use_tree {
            UseTree::Path(use_path) => {
                if let Some(new_tree) =
                    self.remove_top_module_impl(&use_path.tree, crate_name, depth + 1)
                {
                    Some(UseTree::Path(UsePath {
                        ident: use_path.ident.clone(),
                        colon2_token: use_path.colon2_token,
                        tree: Box::new(new_tree),
                    }))
                } else {
                    if depth == 0 {
                        None
                    } else {
                        Some(UseTree::Name(UseName {
                            ident: use_path.ident.clone(),
                        }))
                    }
                }
            }
            // 展開対象のトップレベルのクレートをuse文から削除する
            UseTree::Name(use_name) => {
                if depth == 0
                    && (use_name.ident.to_string() == "crate"
                        || use_name.ident.to_string() == self.package_name)
                {
                    None
                } else if depth == 1 && use_name.ident.to_string() == "self" {
                    None
                } else {
                    Some(use_tree.clone())
                }
            }
            // 空のグループは許容されているのでそのままグループとして返す
            UseTree::Group(use_group) => Some(UseTree::Group(UseGroup {
                brace_token: use_group.brace_token,
                items: use_group
                    .items
                    .iter()
                    .filter_map(|item| self.remove_top_module_impl(item, crate_name, depth))
                    .collect(),
            })),
            UseTree::Rename(_) => Some(use_tree.clone()),
            UseTree::Glob(_) => Some(use_tree.clone()),
        }
    }

    pub fn resolve_modules(&self, use_tree: &UseTree, crate_name: &str) -> UseTree {
        self.resolve_modules_impl(use_tree, crate_name)
    }

    fn resolve_modules_impl(&self, use_tree: &UseTree, crate_name: &str) -> UseTree {
        match use_tree {
            UseTree::Path(use_path) => UseTree::Path(UsePath {
                ident: match use_path.ident {
                    _ if use_path.ident.to_string() == "crate" => {
                        Ident::new(crate_name, use_path.ident.span())
                    }
                    _ => use_path.ident.clone(),
                },
                colon2_token: use_path.colon2_token,
                tree: Box::new(self.resolve_modules_impl(&use_path.tree, crate_name)),
            }),
            // 展開対象のトップレベルのクレートをuse文から削除する
            UseTree::Name(use_name) => {
                if use_name.ident.to_string() == "crate" {
                    UseTree::Name(UseName {
                        ident: Ident::new(crate_name, use_name.ident.span()),
                    })
                } else {
                    use_tree.clone()
                }
            }
            UseTree::Group(use_group) => UseTree::Group(UseGroup {
                brace_token: use_group.brace_token,
                items: use_group
                    .items
                    .iter()
                    .map(|item| self.resolve_modules_impl(item, crate_name))
                    .collect(),
            }),
            UseTree::Rename(_) => use_tree.clone(),
            UseTree::Glob(_) => use_tree.clone(),
        }
    }
}

#[derive(Debug)]
pub struct ModuleNode {
    pub path: Option<PathBuf>,
    pub replacement_spans: Vec<ReplacementSpan>,
    pub children: HashMap<String, ModuleNode>,
}

#[derive(Debug)]
pub struct ReplacementSpan {
    pub start: LineColumn,
    pub end: LineColumn,
    pub replacement: String,
}

impl ModuleNode {
    pub fn new() -> ModuleNode {
        ModuleNode {
            path: None,
            replacement_spans: Vec::new(),
            children: HashMap::new(),
        }
    }

    /// モジュールのノードを再帰的に追加して末尾要素にファイルのパスを登録する
    ///
    /// パスが既に登録されている場合は返戻値として`Some(source_path)`が返される
    pub fn update(&mut self, module_parts: &[String], source_path: PathBuf) -> Option<PathBuf> {
        if module_parts.is_empty() {
            match self.path {
                Some(_) => Some(source_path),
                None => {
                    self.path = Some(source_path);
                    None
                }
            }
        } else {
            let child = self
                .children
                .entry(module_parts[0].clone())
                .or_insert(ModuleNode::new());

            child.update(&module_parts[1..], source_path)
        }
    }

    /// 置換用のスパンの配列を取得する
    pub fn replacement_spans(&self, module_parts: &[String]) -> Option<&[ReplacementSpan]> {
        if module_parts.is_empty() {
            Some(&self.replacement_spans)
        } else {
            if let Some(child) = self.children.get(&module_parts[0]) {
                child.replacement_spans(&module_parts[1..])
            } else {
                None
            }
        }
    }

    /// 変更可能な置換用のスパンの動的配列を取得する
    pub fn replacement_spans_mut(
        &mut self,
        module_parts: &[String],
    ) -> Option<&mut Vec<ReplacementSpan>> {
        if module_parts.is_empty() {
            Some(&mut self.replacement_spans)
        } else {
            if let Some(child) = self.children.get_mut(&module_parts[0]) {
                child.replacement_spans_mut(&module_parts[1..])
            } else {
                None
            }
        }
    }

    /// 置換用のスパンの配列を行数列数の早い順にソートする
    pub fn sort_replacement_spans(&mut self) {
        self.replacement_spans
            .sort_unstable_by_key(|span| span.start);
    }
}
