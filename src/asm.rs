use std::fs;
use std::path::Path;
use std::process;
use regex::Regex;

#[derive(Debug, Clone)]
struct ShaderInfo {
    header: Vec<String>,
    input_signature: Vec<String>,
    output_signature: Vec<String>,
    declarations: Vec<String>,
    code_lines: Vec<String>,
    input_registers: Vec<InputRegister>,
    temp_reg_count: usize,
}

#[derive(Debug, Clone)]
struct InputRegister {
    name: String,
    index: usize,
    mask: String,
    reg: usize,
}

pub fn process_shader_content(content: &str) -> Result<String, Box<dyn std::error::Error>> {
    let shader_info = parse_shader_info(&content)?;
    let texcoord1_reg = find_texcoord1_register(&shader_info.input_registers)?;
    let new_temp_reg = shader_info.temp_reg_count;
    let fixed_code = fix_shader_code(&shader_info, texcoord1_reg, new_temp_reg)?;
    Ok(fixed_code)
}

fn find_texcoord1_register(registers: &[InputRegister]) -> Result<usize, Box<dyn std::error::Error>> {
    for reg in registers {
        if reg.name == "TEXCOORD" && reg.index == 1 {
            println!("Found TEXCOORD 1 at register: v{}", reg.reg);
            return Ok(reg.reg);
        }
    }
    Err("TEXCOORD 1 not found in input signature".into())
}

fn fix_shader_code(shader_info: &ShaderInfo, texcoord1_reg: usize, new_temp_reg: usize) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();
    // 添加文件头
    for line in &shader_info.header {
        output.push_str(line);
        output.push('\n');
    };
    // 添加输入签名
    for line in &shader_info.input_signature {
        output.push_str(line);
        output.push('\n');
    };
    // 添加输出签名
    for line in &shader_info.output_signature {
        output.push_str(line);
        output.push('\n');
    };
    // 处理声明部分，更新dcl_temps行
    for line in &shader_info.declarations {
        if !line.starts_with("dcl_temps") {
            output.push_str(line);
            output.push('\n');
        }
    };
    // 更新临时寄存器数量
    output.push_str(&format!("dcl_temps {}\n", shader_info.temp_reg_count + 1));
    // 添加修复代码
    output.push_str(&format!("mul r{}.z, v{}.x, l(15.937500)\n", new_temp_reg, texcoord1_reg));
    output.push_str(&format!("frc r{}.x, r{}.z\n", new_temp_reg, new_temp_reg));
    output.push_str(&format!("round_ni r{}.w, r{}.z\n", new_temp_reg, new_temp_reg));
    output.push_str(&format!("mul r{}.y, r{}.w, l(0.062500)\n", new_temp_reg, new_temp_reg));
    // 替换所有对目标寄存器的引用
    let pattern = format!(r"v{}.", texcoord1_reg);
    let replacement = format!("r{}.", new_temp_reg);
    println!("replacing {} => {}", pattern, replacement);
    // 处理主程序代码
    for line in &shader_info.code_lines {
        let fixed_line = line.replace(&pattern, &replacement);
        output.push_str(&fixed_line);
        output.push('\n');
    }
    Ok(output)
}

fn parse_shader_info(content: &str) -> Result<ShaderInfo, Box<dyn std::error::Error>> {
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    
    let mut header = Vec::new();
    let mut input_signature = Vec::new();
    let mut output_signature = Vec::new();
    let mut declarations = Vec::new();
    let mut code_lines = Vec::new();
    
    let mut input_registers = Vec::new();
    let mut temp_reg_count = 0;
    let mut current_section = "header";
    
    // let input_regex = Regex::new(r"TEXCOORD\s+(\d+)\s+(\w+)\s+(\d+)\s+")?;
    let input_regex = Regex::new(r"^\/\/\s(\w+)\s+(\d+)\s+(\w+)\s+(\d+)\s+")?;
    let temps_regex = Regex::new(r"dcl_temps\s+(\d+)")?;
    
    // 解析着色器结构
    for line in lines.iter() {
        match current_section {
            "header" => {
                if line.contains("Input signature:") {
                    current_section = "input_signature";
                } else if !line.starts_with("//") {
                    current_section = "declarations";
                }
            }
            "input_signature" => {
                // 解析输入寄存器
                if let Some(caps) = input_regex.captures(line) {
                    let registers = InputRegister {
                        name: caps[1].to_string(),
                        index: caps[2].parse()?,
                        mask: caps[3].to_string(),
                        reg: caps[4].parse()?,
                    };
                    input_registers.push(registers);
                };
                if line.contains("Output signature:") {
                    current_section = "output_signature";
                } else if !line.starts_with("//") {
                    current_section = "declarations";
                }
            }
            "output_signature" => {
                if !line.starts_with("//") {
                    current_section = "declarations";
                }
            }
            "declarations" => {
                // 检查是否是dcl_temps行
                if let Some(caps) = temps_regex.captures(line) {
                    temp_reg_count = caps[1].parse()?;
                };
                // 如果遇到代码行（非声明），切换到代码段
                if !line.starts_with("dcl_") && !line.starts_with("vs_") && !line.starts_with("ps_") {
                    current_section = "code";
                }
            }
            "code" => {}
            _ => {}
        }
        match current_section {
            "header" => header.push(line.clone()),
            "input_signature" => input_signature.push(line.clone()),
            "output_signature" => output_signature.push(line.clone()),
            "declarations" => declarations.push(line.clone()),
            "code" => code_lines.push(line.clone()),
            _ => {}
        }
    }
    Ok(ShaderInfo {
        header,
        input_signature,
        output_signature,
        declarations,
        code_lines,
        input_registers,
        temp_reg_count,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        // 使用示例
        println!("Shader-fixer is a DXBC assembly parser for D3D_SM50");
        println!("Usage: {} input_file.asm output_file.asm", &args[0]);
        process::exit(1);
    };
    // let input_file = "0.Direct3D_SM50.Vertex.asm";
    let input_file = &args[1];
    if !Path::new(input_file).exists() {
        println!("Input file not found. Running in test mode...");
        process::exit(1);
    };
    let output_file = if args.len() == 3 { &args[2] } else { input_file };
    let content = fs::read_to_string(input_file)?;
    match process_shader_content(&content) {
        Ok(processed) => {
            fs::write(output_file, &processed)?;
            println!("Shader processed successfully! Output: {}", output_file);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
    Ok(())
}