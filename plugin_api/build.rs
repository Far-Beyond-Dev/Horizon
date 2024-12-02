use std::fs::{self, File};
use std::path::Path;
use std::io::{Write, Read};

fn main() {
    // Get the path to the plugins directory
    let plugins_dir = Path::new("..").join("plugins");
    
    println!("cargo:warning=Looking for plugins in: {:?}", plugins_dir);
    
    // Ensure the plugins directory exists
    if !plugins_dir.exists() {
        println!("cargo:warning=Plugins directory not found at {:?}", plugins_dir);
        return;
    }

    // Find all valid plugin directories
    let plugin_paths = discover_plugins(&plugins_dir);
    println!("cargo:warning=Found {} plugins", plugin_paths.len());
    
    // Update Cargo.toml with plugin dependencies
    if let Err(e) = update_cargo_toml(&plugin_paths) {
        println!("cargo:warning=Failed to update Cargo.toml: {}", e);
        std::process::exit(1);
    }
    
    // Generate the plugin macro and imports files
    if let Err(e) = generate_plugin_files(&plugin_paths) {
        println!("cargo:warning=Failed to generate plugin files: {}", e);
        std::process::exit(1);
    }
    
    // Tell Cargo to rerun this script if the plugins directory or Cargo.toml changes
    println!("cargo:rerun-if-changed=../plugins");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn discover_plugins(plugins_dir: &Path) -> Vec<(String, String, String)> {
    let mut valid_plugins = Vec::new();
    
    if let Ok(entries) = fs::read_dir(plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            // Check if this is a directory
            if !path.is_dir() {
                continue;
            }
            
            let plugin_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
                
            // Skip if empty plugin name
            if plugin_name.is_empty() {
                continue;
            }
            
            // Check for required files/directories
            let cargo_toml = path.join("Cargo.toml");
            let src_dir = path.join("src");
            let lib_rs = path.join("src").join("lib.rs");
            
            if cargo_toml.exists() && src_dir.exists() && lib_rs.exists() {
                // Read the Cargo.toml to get the package name and version
                if let Ok(mut file) = File::open(&cargo_toml) {
                    let mut contents = String::new();
                    if file.read_to_string(&mut contents).is_ok() {
                        // Simple parsing for package name and version
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
                            println!("cargo:warning=Found plugin: {} v{} in {}", name, version, plugin_name);
                            valid_plugins.push((name, version, plugin_name));
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

fn update_cargo_toml(plugin_paths: &[(String, String, String)]) -> std::io::Result<()> {
    let cargo_path = "Cargo.toml";
    let mut contents = String::new();
    File::open(cargo_path)?.read_to_string(&mut contents)?;

    // Normalize line endings to \n for consistent processing
    contents = contents.replace("\r\n", "\n");

    // Find the boundaries of the auto-generated section
    let start_idx = contents.find(AUTO_GENERATED_START);
    let end_idx = contents.find(AUTO_GENERATED_END);

    let base_contents = match (start_idx, end_idx) {
        (Some(start), Some(end)) => {
            // If an existing section is found, take everything before it
            contents[..start].trim_end().to_string()
        }
        _ => {
            // If no section exists, use all current contents
            contents.trim_end().to_string()
        }
    };

    // Generate the new dependencies section
    let mut new_section = String::new();
    new_section.push('\n');  // Add a newline before the section
    new_section.push_str(AUTO_GENERATED_START);
    new_section.push('\n');  // Add newline after start marker
    
    // Sort plugins by name for consistent output
    let mut sorted_plugins = plugin_paths.to_vec();
    sorted_plugins.sort_by(|a, b| a.0.cmp(&b.0));
    
    for (name, version, plugin_dir) in sorted_plugins {
        new_section.push_str(&format!(
            "{} = {{ path = \"../plugins/{}\", version = \"{}\" }}\n",
            name, plugin_dir, version
        ));
    }
    
    new_section.push_str(AUTO_GENERATED_END);

    // Combine the base contents with the new section
    let mut final_contents = base_contents;
    final_contents.push_str(&new_section);

    // Ensure file ends with a single newline
    if !final_contents.ends_with('\n') {
        final_contents.push('\n');
    }

    // Write the updated Cargo.toml
    fs::write(cargo_path, final_contents)?;
    
    Ok(())
}

fn generate_plugin_files(plugin_paths: &[(String, String, String)]) -> std::io::Result<()> {
    // Create the output directory if it doesn't exist
    let out_dir = Path::new("src");
    fs::create_dir_all(out_dir)?;
    
    // Then generate the imports file that uses the macro
    generate_imports_file(plugin_paths, out_dir)?;
    
    Ok(())
}

fn generate_imports_file(plugin_paths: &[(String, String, String)], out_dir: &Path) -> std::io::Result<()> {
    let mut file = fs::File::create(out_dir.join("plugin_imports.rs"))?;
    
    // Write the header
    writeln!(file, "// This file is automatically generated by build.rs")?;
    writeln!(file, "// Do not edit this file manually!\n")?;
    writeln!(file, "use horizon_plugin_api::LoadedPlugin;")?;
    writeln!(file, "use std::collections::HashMap;\n")?;
    
    // Use the macro with discovered plugins
    writeln!(file, "// Invoke the macro with all discovered plugins")?;
    writeln!(file, "pub fn load_plugins() -> HashMap<&'static str, LoadedPlugin> {{")?;
    write!(file, "    let plugins = crate::load_plugins!(")?;
    
    // Add each plugin to the macro invocation
    for (i, (name, _, _)) in plugin_paths.iter().enumerate() {
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