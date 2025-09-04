use std::cmp::max;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::Path;
use std::process;
use std::sync::Mutex;
use std::sync::OnceLock;

struct Index {
    // 存储所有出现过的输入
    inputs: HashSet<String>,
    // 存储映射
    map: HashMap<String, Vec<(String, String, String)>>,
}

impl Index {
    fn new() -> Self {
        Index {
            inputs: HashSet::new(),
            map: HashMap::new(),
        }
    }

    // 添加一个节点
    fn add_node(&mut self, file_name: &str, file_title: &str, extra_info: &str, input: Vec<&str>) {
        for i in input {
            // 清理i（去除前后空格，转为小写）
            let normalized_i = i.trim().to_lowercase();

            if !normalized_i.is_empty() {
                // 添加到所有i集合
                self.inputs.insert(normalized_i.clone());

                // 添加到i到节点的映射
                self.map.entry(normalized_i).or_default().push((
                    file_name.to_string(),
                    file_title.to_string(),
                    extra_info.to_string(),
                ));
            }
        }
    }

    // 根据i获取节点名字列表
    fn get_files_by_i(&self, i: &str) -> Option<&Vec<(String, String, String)>> {
        let normalized_i = i.trim().to_lowercase();
        self.map.get(&normalized_i)
    }

    // 获取i对应的节点数量
    fn get_i_count(&self, i: &str) -> usize {
        let normalized_i = i.trim().to_lowercase();
        self.map.get(&normalized_i).map_or(0, |files| files.len())
    }

    // 获取所有出现过的i名称
    fn get_inputs(&self) -> &HashSet<String> {
        &self.inputs
    }
}

struct ColumnFormatter {
    columns_per_row: usize,
    column_padding: usize,
}

impl ColumnFormatter {
    fn new(columns_per_row: usize) -> Self {
        Self {
            columns_per_row,
            column_padding: 2, // 默认列间距
        }
    }

    fn with_padding(mut self, padding: usize) -> Self {
        self.column_padding = padding;
        self
    }

    fn format(&self, input: &str) -> String {
        let words: Vec<&str> = input.split_whitespace().collect();

        if words.is_empty() {
            return String::new();
        }

        // 计算每列最大宽度
        let mut col_widths = vec![0; self.columns_per_row];

        for (i, word) in words.iter().enumerate() {
            let col_index = i % self.columns_per_row;
            col_widths[col_index] = max(col_widths[col_index], word.len());
        }

        // 构建输出
        let mut output = String::new();
        let padding_str = " ".repeat(self.column_padding);

        for (i, word) in words.iter().enumerate() {
            let col_index = i % self.columns_per_row;

            // 格式化当前列
            output.push_str(&format!("{:<width$}", word, width = col_widths[col_index]));

            // 添加列间距或换行
            if col_index < self.columns_per_row - 1 {
                output.push_str(&padding_str);
            } else {
                output.push('\n');
            }
        }

        // 确保最后有换行
        if !output.ends_with('\n') {
            output.push('\n');
        }

        output
    }
}

static GLOBAL_DATES: OnceLock<Mutex<Index>> = OnceLock::new();
static GLOBAL_TAGS: OnceLock<Mutex<Index>> = OnceLock::new();

fn get_global_tags() -> &'static Mutex<Index> {
    GLOBAL_TAGS.get_or_init(|| Mutex::new(Index::new()))
}

fn get_global_dates() -> &'static Mutex<Index> {
    GLOBAL_DATES.get_or_init(|| Mutex::new(Index::new()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 获取命令行参数
    let args: Vec<String> = env::args().collect();

    // 参数数量检查（第一个参数是程序名）
    if args.len() > 2 {
        eprintln!("使用方法: {} <目录路径>", args[0]);
        std::process::exit(1);
    }

    let dir_path = if args.len() == 1 {
        &format!(
            "{}/.data",
            &match env::var("HOME") {
                Ok(val) => val,
                Err(e) => {
                    eprintln!("无法获取 HOME 环境变量: {}", e);
                    std::process::exit(1);
                }
            }
        )
    } else {
        &args[1]
    };

    let path = Path::new(dir_path);
    let tag_index = get_global_tags();
    let date_index = get_global_dates();

    // 检查路径是否存在且为目录
    if !path.exists() {
        eprintln!("错误: 路径 '{}' 不存在", dir_path);
        std::process::exit(1);
    }

    if !path.is_dir() {
        eprintln!("错误: '{}' 不是目录", dir_path);
        std::process::exit(1);
    }

    // 读取目录内容
    let entries = fs::read_dir(path).map_err(|e| format!("无法读取目录 '{}': {}", dir_path, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("目录项错误: {}", e))?;
        let file_path = entry.path();

        // 检查是否为.md文件
        if let Some(ext) = file_path.extension()
            && ext == "md"
            && file_path.is_file()
        {
            println!("\n=== 处理文件: {} ===", file_path.display());

            // 读取文件前5行
            if let Err(e) = read_first_5_lines(&file_path) {
                eprintln!("读取文件失败 {}: {}", file_path.display(), e);
            }
        }
    }

    println!("\n索引构建完成！");
    let index_path = path.join("index.md");
    let file = File::create(&index_path)?;
    let mut writer = BufWriter::new(file);
    let header = "---\nTitle: index\n---\n\n# Tags";
    writeln!(writer, "{}", header)?;

    let mut output_tags = String::new();
    let mut tags_data: Vec<(&str, usize)> = Vec::new();
    let tags = tag_index.lock().unwrap();
    // 输出tag的名字和对应含有tag的节点数量
    for tag in tags.get_inputs() {
        let count = tags.get_i_count(tag);
        tags_data.push((tag, count));
        let tag_with_ext = format!("{}.md", tag);
        let tag_path = path.join(tag_with_ext);
        let tag_file = File::create(&tag_path)?;
        let mut tag_writer = BufWriter::new(tag_file);
        writeln!(tag_writer, "---\nTitle: {}\n---\n\n#list", tag)?;
        let file_list = tags.get_files_by_i(tag);
        for (file_name, file_title, _) in file_list.unwrap_or(&Vec::new()) {
            writeln!(tag_writer, "[[{}|{}]]", file_name, file_title)?;
        }
    }
    tags_data.sort_by(|a, b| b.1.cmp(&a.1));
    for (tag, count) in tags_data {
        output_tags.push_str(&format!("{}({}) ", tag, count));
    }
    let formatter = ColumnFormatter::new(5).with_padding(2);
    let result = formatter.format(&output_tags);
    writeln!(writer, "{}", result)?;

    let header = "# Dates";
    writeln!(writer, "{}", header)?;
    let mut output_dates = String::new();
    let mut dates_data: Vec<(usize, usize)> = Vec::new();
    let dates = date_index.lock().unwrap();

    // 显示每个date的节点数量
    for date in dates.get_inputs() {
        let count = dates.get_i_count(date);
        match date.parse::<usize>() {
            Ok(_) => {}
            Err(e) => println!("解析失败: {}", e),
        }
        dates_data.push((date.parse()?, count));
        let date_with_ext = format!("{}.md", date);
        let date_path = path.join(date_with_ext);
        let date_file = File::create(&date_path)?;
        let mut date_writer = BufWriter::new(date_file);
        writeln!(date_writer, "---\nTitle: {}\n---\n\n#list", date)?;
        let mut file_list: Vec<(String, String, String)> =
            (*dates.get_files_by_i(date).unwrap().clone()).to_vec();
        file_list.sort_by(|a, b| a.2.cmp(&b.2));
        // let mut time_data: Vec<(String, String)> = Vec::new();
        // for (file_name, file_title, ltime) in file_list.unwrap_or(&Vec::new()) {
        //     let anchor = format!("[[{}|{}]]", file_name, file_title);
        //     time_data.push((ltime.to_string(), anchor));
        // }
        // time_data.sort_by(|a, b| a.0.cmp(&b.0));
        for (file_name, file_title, ltime) in file_list {
            let output_line = &format!("[[{}|{}|{}]] ", file_name, ltime, file_title);
            writeln!(date_writer, "{}", output_line)?;
        }
    }
    dates_data.sort_by(|a, b| b.0.cmp(&a.0));
    for (date, count) in dates_data {
        output_dates.push_str(&format!("[[{}]]({}) ", date, count));
    }
    let formatter = ColumnFormatter::new(7);
    let result = formatter.format(&output_dates);
    writeln!(writer, "{}", result)?;

    writer.flush()?;

    Ok(())
}

fn read_first_5_lines(file_path: &Path) -> io::Result<()> {
    let file = fs::File::open(file_path)?;
    let file_name = file_path.file_name().unwrap().to_str().unwrap().to_string();
    let file_name_without_ext = &file_name.strip_suffix(".md").unwrap();
    let reader = io::BufReader::new(file);

    let date_index = get_global_dates();
    let tag_index = get_global_tags();
    let mut line_count = 0;
    let mut title = String::new();

    for line in reader.lines() {
        let line = line?;
        if line_count == 1 && line.starts_with("Title: ") {
            title = line.strip_prefix("Title: ").unwrap().to_string();
        }
        if line_count == 2 && line.starts_with("---") {
            match fs::remove_file(file_path) {
                Ok(()) => {
                    println!("成功删除文件: {}", &file_path.display());
                }
                Err(e) => {
                    // 根据错误类型提供更具体的提示
                    match e.kind() {
                        std::io::ErrorKind::NotFound => {
                            eprintln!("错误: 文件不存在 - {}", &file_path.display());
                        }
                        std::io::ErrorKind::PermissionDenied => {
                            eprintln!("错误: 没有删除权限 - {}", &file_path.display());
                        }
                        _ => {
                            eprintln!("删除文件时发生错误: {}", e);
                        }
                    }
                    process::exit(1);
                }
            }
        }
        if line_count == 3 && line.starts_with("Created:") {
            let full_date: Vec<&str> = line
                .strip_prefix("Created:")
                .unwrap()
                .split_whitespace()
                .collect();

            if full_date.is_empty() {
                eprintln!("(没有创建时间)");
                process::exit(1);
            }

            let date = full_date[..1].to_vec();
            let ltime = full_date[1];
            println!("{}", ltime);

            date_index
                .lock()
                .unwrap()
                .add_node(file_name_without_ext, &title, ltime, date);
        }
        if line_count == 4 && line.starts_with("Tags:") {
            let tags: Vec<&str> = line
                .strip_prefix("Tags:")
                .unwrap()
                .split_whitespace()
                .collect();

            if tags.is_empty() {
                eprintln!("(Tags行没有标签)");
                process::exit(1);
            }

            tag_index
                .lock()
                .unwrap()
                .add_node(file_name_without_ext, &title, "", tags);
        }
        line_count += 1;
        if line_count >= 5 {
            break;
        }
    }

    // 如果文件行数不足5行
    if line_count < 5 {
        println!("(文件只有 {} 行)", line_count);
    }

    Ok(())
}
