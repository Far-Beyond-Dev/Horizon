use std::fs::{self, File};
use std::path::Path;
use std::io::{Write, Read};

// Updated to include whether backend has .allow-imports
type BackendInfo = (String, String, String, bool);
type PluginInfo = (String, String, String, bool);


fn main() {
    let backends_dir = Path::new("..").join("backends");
    let plugins_dir = Path::new("..").join("plugins");
    
    println!("cargo:warning=Looking for backends in: {:?}", backends_dir);
    
    if !backends_dir.exists() {
        println!("cargo:warning=Backends directory not found at {:?}", backends_dir);
        return;
    }

    let backend_paths = discover_backends(&backends_dir);
    let plugin_paths = discover_plugins(&plugins_dir);
    println!("cargo:warning=Found {} backends", backend_paths.len());
    
    // Update main Cargo.toml
    if let Err(e) = update_cargo_toml(&backend_paths) {
        println!("cargo:warning=Failed to update Cargo.toml: {}", e);
        std::process::exit(1);
    }
    
    // Update individual backend Cargo.toml files if they have .allow-imports
    if let Err(e) = update_backend_cargo_tomls(&plugins_dir, &plugin_paths, &backends_dir, &backend_paths) {
        println!("cargo:warning=Failed to update backend Cargo.toml files: {}", e);
        std::process::exit(1);
    }
    
    if let Err(e) = generate_backend_files(&backend_paths) {
        println!("cargo:warning=Failed to generate backend files: {}", e);
        std::process::exit(1);
    }
    
    println!("cargo:rerun-if-changed=../backends");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn discover_plugins(plugins_dir: &Path) -> Vec<PluginInfo> {
    // This is identical to discover_backends but returns PluginInfo
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

fn discover_backends(backends_dir: &Path) -> Vec<BackendInfo> {
    let mut valid_backends = Vec::new();
    
    if let Ok(entries) = fs::read_dir(backends_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }
            
            let backend_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
                
            if backend_name.is_empty() {
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
                            println!("cargo:warning=Found backend: {} v{} in {} (allow-imports: {})",
                                   name, version, backend_name, has_allow_imports);
                            valid_backends.push((name, version, backend_name, has_allow_imports));
                        }
                    }
                }
            }
        }
    }
    
    valid_backends
}

const AUTO_GENERATED_START: &str = "###### BEGIN AUTO-GENERATED BACKEND DEPENDENCIES - DO NOT EDIT THIS SECTION ######";
const AUTO_GENERATED_END: &str = "###### END AUTO-GENERATED BACKEND DEPENDENCIES ######";

fn update_cargo_toml(backend_paths: &[BackendInfo]) -> std::io::Result<()> {
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
    
    let mut sorted_backends = backend_paths.to_vec();
    sorted_backends.sort_by(|a, b| a.0.cmp(&b.0));
    
    for (name, version, backend_dir, _) in sorted_backends {
        new_section.push_str(&format!(
            "{} = {{ path = \"../backends/{}\", version = \"{}\" }}\n",
            name, backend_dir, version
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

fn update_backend_cargo_tomls(plugins_dir: &Path, plugin_paths: &[PluginInfo], backends_dir: &Path, backend_paths: &[BackendInfo]) -> std::io::Result<()> {
    println!("cargo:warning=Starting update_backend_cargo_tomls");
    println!("cargo:warning=Found {} plugins and {} backends", plugin_paths.len(), backend_paths.len());
    
    // Update each backend Cargo.toml if it has .allow-imports
    for (name, version, backend_dir, has_allow) in backend_paths.iter() {
        println!("cargo:warning=Checking backend {} (allow_imports: {})", name, has_allow);
        
        if !*has_allow {
            println!("cargo:warning=Skipping backend {} - no .allow-imports", name);
            continue;
        }

        let backend_cargo_path = backends_dir.join(backend_dir).join("Cargo.toml");
        println!("cargo:warning=Updating Cargo.toml at {:?}", backend_cargo_path);
        
        if !backend_cargo_path.exists() {
            println!("cargo:warning=ERROR: Cargo.toml not found at {:?}", backend_cargo_path);
            continue;
        }

        let mut contents = String::new();
        match File::open(&backend_cargo_path) {
            Ok(mut file) => {
                if let Err(e) = file.read_to_string(&mut contents) {
                    println!("cargo:warning=ERROR: Failed to read Cargo.toml: {}", e);
                    continue;
                }
            }
            Err(e) => {
                println!("cargo:warning=ERROR: Failed to open Cargo.toml: {}", e);
                continue;
            }
        }

        contents = contents.replace("\r\n", "\n");

        let start_idx = contents.find(AUTO_GENERATED_START);
        let end_idx = contents.find(AUTO_GENERATED_END);
        
        println!("cargo:warning=Found markers in file: start={:?}, end={:?}", start_idx.is_some(), end_idx.is_some());

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

        // Add dependencies for all plugins
        println!("cargo:warning=Adding {} plugin dependencies", plugin_paths.len());
        for (plugin_name, plugin_version, plugin_dir, _) in plugin_paths.iter() {
            // Don't add self as dependency
            if plugin_name != name {
                let dep_line = format!(
                    "{} = {{ path = \"../../plugins/{}\", version = \"{}\" }}\n",
                    plugin_name, plugin_dir, plugin_version
                );
                println!("cargo:warning=Adding dependency: {}", dep_line.trim());
                new_section.push_str(&dep_line);
            }
        }

        new_section.push_str(AUTO_GENERATED_END);
        new_section.push('\n');

        let mut final_contents = base_contents;
        final_contents.push_str(&new_section);

        if !final_contents.ends_with('\n') {
            final_contents.push('\n');
        }

        match fs::write(&backend_cargo_path, &final_contents) {
            Ok(_) => println!("cargo:warning=Successfully updated {:?}", backend_cargo_path),
            Err(e) => println!("cargo:warning=ERROR: Failed to write to {:?}: {}", backend_cargo_path, e),
        }
    }

    Ok(())
}

fn generate_backend_files(backend_paths: &[BackendInfo]) -> std::io::Result<()> {
    let out_dir = Path::new("src");
    fs::create_dir_all(out_dir)?;
    generate_imports_file(backend_paths, out_dir)?;
    Ok(())
}

fn generate_imports_file(backend_paths: &[BackendInfo], out_dir: &Path) -> std::io::Result<()> {
    let mut file = fs::File::create(out_dir.join("backend_imports.rs"))?;
    
    writeln!(file, "// This file is automatically generated by build.rs")?;
    writeln!(file, "// Do not edit this file manually!\n")?;
    writeln!(file, "use horizon_plugin_api::{{Pluginstate, LoadedPlugin, Plugin}};")?;
    writeln!(file, "use std::collections::HashMap;\n")?;
    
    for (name, _, _, _) in backend_paths {
        write!(file, "pub use {};\n", name)?;
        write!(file, "pub use {}::*;\n", name)?;
        write!(file, "pub use {}::Plugin as {}_backend;\n", name, name)?;
    }
    writeln!(file, "\n")?;

    writeln!(file, "// Invoke the macro with all discovered backends")?;
    writeln!(file, "pub fn load_backends() -> HashMap<String, (Pluginstate, Plugin)> {{")?;
    write!(file, "    let backends = crate::load_backends!(")?;
    
    for (i, (name, _, _, _)) in backend_paths.iter().enumerate() {
        if i > 0 {
            write!(file, ",")?;
        }
        write!(file, "\n        {}", name)?;
    }
    
    writeln!(file, "\n    );")?;
    writeln!(file, "    backends")?;
    writeln!(file, "}}")?;
    
    Ok(())
}