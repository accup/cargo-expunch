use std::collections::HashMap;
use std::path::PathBuf;
use syn::{self, UseTree};

pub fn concat_module_parts(prefix: &[String], suffix: &[String], crate_name: &str) -> Vec<String> {
    let mut full_parts = prefix.to_vec();
    for part in suffix {
        match part {
            _ if part == "crate" || part == crate_name => {
                full_parts.clear();
                full_parts.push(part.clone());
            }
            _ if part == "super" => {
                full_parts.pop();
            }
            _ if part == "self" => {}
            _ => {
                full_parts.push(part.clone());
            }
        }
    }
    full_parts
}

#[derive(Debug, Clone)]
pub enum ModuleItemAccessibility {
    Direct(ModuleItemPath),
    Indirect(ModuleItemPath),
}

#[derive(Debug, Clone)]
pub enum ModuleItemPath {
    Dir(Vec<String>, PathBuf),
    File(Vec<String>, PathBuf),
    Insoluble(Vec<String>),
}

pub fn make_module_item_path(
    module_parts: &[String],
    package_name: &str,
    package_src_path: &PathBuf,
    crate_path: &PathBuf,
    self_path: &PathBuf,
) -> Result<ModuleItemPath, String> {
    let mut lib_file = None;
    let mut path_buf = PathBuf::new();

    let resolved_parts = module_parts
        .iter()
        .filter_map(|module_part| match module_part {
            _ if module_part == "crate" => None,
            _ if module_part == "self" => None,
            _ => Some(String::from(module_part)),
        })
        .collect();

    for module_part in module_parts.iter() {
        lib_file = None;
        path_buf.push(match module_part {
            _ if module_part == "crate" => crate_path.clone(),
            _ if module_part == package_name => {
                lib_file = Some(package_src_path.join("lib.rs"));
                package_src_path.clone()
            }
            _ if module_part == "super" => self_path
                .parent()
                .ok_or_else(|| {
                    format!(
                        "Failed to get the parent directory of the {0}
{1} より上の階層へ遡ろうとしました",
                        self_path.to_str().unwrap_or("(undisplayable path)"),
                        self_path.to_str().unwrap_or("（表示できないパス）"),
                    )
                })?
                .to_path_buf(),
            _ if module_part == "self" => {
                if path_buf.as_os_str().is_empty() {
                    self_path.clone()
                } else {
                    continue;
                }
            }
            _ => PathBuf::from(module_part),
        });
    }

    let module_name_file = path_buf.with_extension("rs");

    Ok(
        if let Some(lib_file) =
            lib_file.and_then(|file| if file.is_file() { Some(file) } else { None })
        {
            ModuleItemPath::File(resolved_parts, lib_file)
        } else if module_name_file.is_file() {
            ModuleItemPath::File(resolved_parts, module_name_file)
        } else if path_buf.is_dir() {
            let mod_file = path_buf.join("mod.rs");
            if mod_file.is_file() {
                ModuleItemPath::File(resolved_parts, mod_file)
            } else {
                ModuleItemPath::Dir(resolved_parts, path_buf)
            }
        } else {
            ModuleItemPath::Insoluble(resolved_parts)
        },
    )
}

pub fn collect_module_items(
    use_tree: &UseTree,
    package_name: &str,
    package_src_path: &PathBuf,
    crate_name: &str,
    crate_path: &PathBuf,
    self_path: &PathBuf,
) -> Result<Vec<ModuleItemAccessibility>, String> {
    let mut module_path_map = HashMap::new();
    collect_module_items_impl(
        use_tree,
        &mut Vec::new(),
        package_name,
        package_src_path,
        crate_name,
        crate_path,
        self_path,
        &mut module_path_map,
    )?;
    Ok(module_path_map.values().cloned().collect())
}

fn collect_module_items_impl(
    use_tree: &UseTree,
    module_parts: &mut Vec<String>,
    package_name: &str,
    package_src_path: &PathBuf,
    crate_name: &str,
    crate_path: &PathBuf,
    self_path: &PathBuf,
    module_path_map: &mut HashMap<Vec<String>, ModuleItemAccessibility>,
) -> Result<(), String> {
    match use_tree {
        UseTree::Path(use_path) => {
            let name = use_path.ident.to_string();
            module_parts.push(name);

            module_path_map.entry(module_parts.clone()).or_insert(
                ModuleItemAccessibility::Indirect(make_module_item_path(
                    module_parts,
                    package_name,
                    package_src_path,
                    crate_path,
                    self_path,
                )?),
            );

            collect_module_items_impl(
                &use_path.tree,
                module_parts,
                package_name,
                package_src_path,
                crate_name,
                crate_path,
                self_path,
                module_path_map,
            )?;

            module_parts.pop();
        }
        UseTree::Name(use_name) => {
            let name = use_name.ident.to_string();
            module_parts.push(name);

            module_path_map
                .entry(module_parts.clone())
                .or_insert(ModuleItemAccessibility::Direct(make_module_item_path(
                    module_parts,
                    package_name,
                    package_src_path,
                    crate_path,
                    self_path,
                )?));

            module_parts.pop();
        }
        UseTree::Rename(use_rename) => {
            let name = use_rename.ident.to_string();
            module_parts.push(name);

            module_path_map
                .entry(module_parts.clone())
                .or_insert(ModuleItemAccessibility::Direct(make_module_item_path(
                    module_parts,
                    package_name,
                    package_src_path,
                    crate_path,
                    self_path,
                )?));

            module_parts.pop();
        }
        UseTree::Group(use_group) => {
            for item in use_group.items.iter() {
                collect_module_items_impl(
                    item,
                    module_parts,
                    package_name,
                    package_src_path,
                    crate_name,
                    crate_path,
                    self_path,
                    module_path_map,
                )?;
            }
        }
        _ => (),
    };
    Ok(())
}
