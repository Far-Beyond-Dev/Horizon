use std::fs::{self, File};
use std::path::Path;
use std::io::{Write, Read};

// Updated to include whether plugin has .allow-imports
type PluginInfo = (String, String, String, bool);

fn main() {
    let plugins_dir = Path::new("..").join("plugins");
    
    println!("cargo:warning=Looking for plugins in: {:?}", plugins_dir);
    
    if !plugins_dir.exists() {
        println!("cargo:warning=Plugins directory not found at {:?}", plugins_dir);
        return;
    }

    let plugin_paths = discover_plugins(&plugins_dir);
    println!("cargo:warning=Found {} plugins", plugin_paths.len());
    
    // Update main Cargo.toml
    if let Err(e) = update_cargo_toml(&plugin_paths) {
        println!("cargo:warning=Failed to update Cargo.toml: {}", e);
        std::process::exit(1);
    }
    
    // Update individual plugin Cargo.toml files if they have .allow-imports
    if let Err(e) = update_plugin_cargo_tomls(&plugins_dir, &plugin_paths) {
        println!("cargo:warning=Failed to update plugin Cargo.toml files: {}", e);
        std::process::exit(1);
    }
    
    if let Err(e) = generate_plugin_files(&plugin_paths) {
        println!("cargo:warning=Failed to generate plugin files: {}", e);
        std::process::exit(1);
    }
    
    println!("cargo:rerun-if-changed=../plugins");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn discover_plugins(plugins_dir: &Path) -> Vec<PluginInfo> {
    let mut valid_plugins = Vec::new();
    
    if let Ok(entries) = fs::read_dir(plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }
            
            let plugin_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
                
            if plugin_name.is_empty() {
                continue;
            }
            
            let cargo_toml = path.join("Cargo.toml");
            let src_dir = path.join("src");
            let lib_rs = path.join("src").join("lib.rs");
            let allow_imports = path.join(".allow-imports");
            
            if cargo_toml.exists() && src_dir.exists() && lib_rs.exists() {
                if let Ok(mut file) = File::open(&cargo_toml) {
                    let mut contents = String::new();
                    if file.read_to_string(&mut contents).is_ok() {
                        let mut name = None;
                        let mut version = None;
                        
                        for line in contents.lines() {
                            let line = line.trim();
                            if line.starts_with("name") {
                                name = line.split('=')
                                    .nth(1)
                                    .map(|s| s.trim().trim_matches('"').to_string());
                            } else if line.starts_with("version") {
                                version = line.split('=')
                                    .nth(1)
                                    .map(|s| s.trim().trim_matches('"').to_string());
                            }
                        }
                        
                        if let (Some(name), Some(version)) = (name, version) {
                            let has_allow_imports = allow_imports.exists();
                            println!("cargo:warning=Found plugin: {} v{} in {} (allow-imports: {})",
                                   name, version, plugin_name, has_allow_imports);
                            valid_plugins.push((name, version, plugin_name, has_allow_imports));
                        }
                    }
                }
            }
        }
    }
    
    valid_plugins
}

const AUTO_GENERATED_START: &str = "###### BEGIN AUTO-GENERATED PLUGIN DEPENDENCIES - DO NOT EDIT THIS SECTION ######";
const AUTO_GENERATED_END: &str = "###### END AUTO-GENERATED PLUGIN DEPENDENCIES ######";

fn update_cargo_toml(plugin_paths: &[PluginInfo]) -> std::io::Result<()> {
    let cargo_path = "Cargo.toml";
    let mut contents = String::new();
    File::open(cargo_path)?.read_to_string(&mut contents)?;

    contents = contents.replace("\r\n", "\n");

    let start_idx = contents.find(AUTO_GENERATED_START);
    let end_idx = contents.find(AUTO_GENERATED_END);

    let base_contents = match (start_idx, end_idx) {
        (Some(start), Some(end)) => {
            contents[..start].trim_end().to_string()
        }
        _ => {
            contents.trim_end().to_string()
        }
    };

    let mut new_section = String::new();
    new_section.push('\n');
    new_section.push_str(AUTO_GENERATED_START);
    new_section.push('\n');
    
    let mut sorted_plugins = plugin_paths.to_vec();
    sorted_plugins.sort_by(|a, b| a.0.cmp(&b.0));
    
    for (name, version, plugin_dir, _) in sorted_plugins {
        new_section.push_str(&format!(
            "{} = {{ path = \"../plugins/{}\", version = \"{}\" }}\n",
            name, plugin_dir, version
        ));
    }
    
    new_section.push_str(AUTO_GENERATED_END);

    let mut final_contents = base_contents;
    final_contents.push_str(&new_section);

    if !final_contents.ends_with('\n') {
        final_contents.push('\n');
    }

    fs::write(cargo_path, final_contents)?;
    
    Ok(())
}

fn update_plugin_cargo_tomls(plugins_dir: &Path, plugin_paths: &[PluginInfo]) -> std::io::Result<()> {
    // Get list of plugins that don't have .allow-imports
    let regular_plugins: Vec<_> = plugin_paths.iter()
        .filter(|(_, _, _, has_allow)| !has_allow)
        .collect();

    // Update each plugin that has .allow-imports
    for (name, version, plugin_dir, has_allow) in plugin_paths {
        if !has_allow {
            continue;
        }

        let plugin_cargo_path = plugins_dir.join(plugin_dir).join("Cargo.toml");
        let mut contents = String::new();
        File::open(&plugin_cargo_path)?.read_to_string(&mut contents)?;

        contents = contents.replace("\r\n", "\n");

        let start_idx = contents.find(AUTO_GENERATED_START);
        let end_idx = contents.find(AUTO_GENERATED_END);

        let base_contents = match (start_idx, end_idx) {
            (Some(start), Some(end)) => {
                contents[..start].trim_end().to_string()
            }
            _ => {
                contents.trim_end().to_string()
            }
        };

        let mut new_section = String::new();
        new_section.push('\n');
        new_section.push_str(AUTO_GENERATED_START);
        new_section.push('\n');

        // Add dependencies for all non-.allow-imports plugins
        for (dep_name, dep_version, dep_dir, _) in &regular_plugins {
            if dep_name != name {  // Don't add self as dependency
                new_section.push_str(&format!(
                    "{} = {{ path = \"../{}\", version = \"{}\" }}\n",
                    dep_name, dep_dir, dep_version
                ));
            }
        }

        new_section.push_str(AUTO_GENERATED_END);

        let mut final_contents = base_contents;
        final_contents.push_str(&new_section);

        if !final_contents.ends_with('\n') {
            final_contents.push('\n');
        }

        fs::write(plugin_cargo_path, final_contents)?;
    }

    Ok(())
}

fn generate_plugin_files(plugin_paths: &[PluginInfo]) -> std::io::Result<()> {
    let out_dir = Path::new("src");
    fs::create_dir_all(out_dir)?;
    generate_imports_file(plugin_paths, out_dir)?;
    Ok(())
}

fn generate_imports_file(plugin_paths: &[PluginInfo], out_dir: &Path) -> std::io::Result<()> {
    let mut file = fs::File::create(out_dir.join("plugin_imports.rs"))?;
    
    writeln!(file, "// This file is automatically generated by build.rs")?;
    writeln!(file, "// Do not edit this file manually!\n")?;
    writeln!(file, "use horizon_plugin_api::{{Pluginstate, LoadedPlugin, Plugin}};")?;
    writeln!(file, "use std::collections::HashMap;\n")?;
    
    for (name, _, _, _) in plugin_paths {
        write!(file, "pub use {};\n", name)?;
        write!(file, "pub use {}::*;\n", name)?;
        write!(file, "pub use {}::Plugin as {}_plugin;\n", name, name)?;
    }
    writeln!(file, "\n")?;

    writeln!(file, "// Invoke the macro with all discovered plugins")?;
    writeln!(file, "pub fn load_plugins() -> HashMap<String, (Pluginstate, Plugin)> {{")?;
    write!(file, "    let plugins = crate::load_plugins!(")?;
    
    for (i, (name, _, _, _)) in plugin_paths.iter().enumerate() {
        if i > 0 {
            write!(file, ",")?;
        }
        write!(file, "\n        {}", name)?;
    }
    
    writeln!(file, "\n    );")?;
    writeln!(file, "    plugins")?;
    writeln!(file, "}}")?;
    
    Ok(())
}