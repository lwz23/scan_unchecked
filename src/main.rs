use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use syn::{ItemFn, ItemImpl, visit::{self, Visit}, parse_file, ImplItem};

struct FunctionVisitor {
    unchecked_functions: HashSet<(String, String)>, // 存储 (文件路径, 函数名)
    current_file: String,
}

impl<'ast> Visit<'ast> for FunctionVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let fn_name = node.sig.ident.to_string();
        let current_file = self.current_file.clone();

        if fn_name.contains("unchecked") {
            self.unchecked_functions.insert((current_file, fn_name));
        }

        visit::visit_item_fn(self, node); // 遍历函数的其他部分
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        // 遍历 impl 中的所有函数
        for item in &node.items {
            if let ImplItem::Fn(item_fn) = item {
                let method_name = item_fn.sig.ident.to_string();
                let current_file = self.current_file.clone();

                if method_name.contains("unchecked") {
                    self.unchecked_functions.insert((current_file, method_name));
                }
            }
        }
        visit::visit_item_impl(self, node); // 继续遍历 impl 结构的其他部分
    }
}

fn process_file(file_path: &str, unchecked_functions: &Arc<Mutex<HashSet<(String, String)>>>) -> Result<()> {
    let file_content = fs::read_to_string(file_path)?; // 读取文件内容
    let parsed_file = parse_file(&file_content)?; // 解析 Rust 文件

    // 创建一个函数访问者
    let mut visitor = FunctionVisitor {
        unchecked_functions: HashSet::new(),
        current_file: file_path.to_string(), // 设置当前文件路径
    };

    // 遍历文件中的所有项
    visitor.visit_file(&parsed_file);

    // 将找到的 unchecked 函数记录到输出集合中
    let mut output = unchecked_functions.lock().unwrap();
    for func in visitor.unchecked_functions {
        output.insert(func);
    }

    Ok(())
}

fn process_directory(dir_path: &str, unchecked_functions: &Arc<Mutex<HashSet<(String, String)>>>) -> Result<()> {
    let paths: Vec<_> = fs::read_dir(dir_path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();

    for path in paths {
        if path.is_dir() {
            process_directory(path.to_str().unwrap(), unchecked_functions)?; // 递归处理目录
        } else if let Some(ext) = path.extension() {
            if ext == "rs" {
                let path_display = path.display().to_string();
                println!("Processing file: {}", path_display);
                process_file(&path_display, unchecked_functions)?; // 处理 Rust 文件
            }
        }
    }

    Ok(())
}

fn check_for_safe_versions(
    unchecked_functions: Arc<Mutex<HashSet<(String, String)>>>,
) -> Result<HashSet<(String, String, String)>> {
    let mut results = HashSet::<(String, String, String)>::new();
    let output = unchecked_functions.lock().unwrap();

    for (file_path, func_name) in output.iter() {
        // 生成安全版本的函数名
        let safe_func_name = func_name.replace("_unchecked", "");

        // 读取文件内容
        let file_content = fs::read_to_string(file_path)?;
        let parsed_file = parse_file(&file_content)?;

        let mut found_safe_func = false;

        // 遍历文件中的所有项，查找具有相同名称的安全版本函数
        for item in parsed_file.items {
            match item {
                syn::Item::Fn(item_fn) => {
                    if item_fn.sig.ident.to_string() == safe_func_name {
                        found_safe_func = true;
                        break;
                    }
                }
                syn::Item::Impl(item_impl) => {
                    // 遍历 impl 块中的所有方法
                    for impl_item in item_impl.items {
                        if let ImplItem::Fn(impl_fn) = impl_item {
                            if impl_fn.sig.ident.to_string() == safe_func_name {
                                found_safe_func = true;
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // 根据查找结果更新结果集
        if found_safe_func {
            results.insert((file_path.clone(), func_name.clone(), safe_func_name));
        } else {
            results.insert((file_path.clone(), func_name.clone(), "None".to_string()));
        }
    }

    Ok(results)
}


fn main() -> Result<()> {
    let crate_dir = r"library"; // 替换为你的 Rust 标准库路径

    let unchecked_functions = Arc::new(Mutex::new(HashSet::<(String, String)>::new()));

    process_directory(crate_dir, &unchecked_functions)?; // 开始扫描指定目录

    // 检查未检查函数是否对应有安全版本
    let safe_version_results = check_for_safe_versions(unchecked_functions)?;

    // 计算最大宽度
    let max_file_path_len = safe_version_results.iter().map(|(path, _, _)| path.len()).max().unwrap_or(0);
    let max_unchecked_func_len = safe_version_results.iter().map(|(_, func, _)| func.len()).max().unwrap_or(0);
    let max_safe_func_len = safe_version_results.iter().map(|(_, _, safe_func)| safe_func.len()).max().unwrap_or(0);
    
    // 将检查结果输出到文件
    let mut file = File::create("safe_version_results.txt")?;
    writeln!(file, "| {:a$} | {:b$} | {:c$} |", "File Path", "Unchecked Function", "Safe Function", 
             a=max_file_path_len+2, b=max_unchecked_func_len+2, c=max_safe_func_len+2)?;
    writeln!(file, "|{:-<a$}|{:-<b$}|{:-<c$}|", "", "", "", 
             a=max_file_path_len+2, b=max_unchecked_func_len+2, c=max_safe_func_len+2)?;
             
    for (file_path, unchec_func, safe_func) in safe_version_results {
        writeln!(file, "| {:a$} | {:b$} | {:c$} |", file_path, unchec_func, safe_func, 
                 a=max_file_path_len+2, b=max_unchecked_func_len+2, c=max_safe_func_len+2)?; // 写入结果
    }

    println!("Safe version results have been written to safe_version_results.txt");

    Ok(())
}
