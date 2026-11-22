use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use crate::duplicates::{deduplicate_with_hardlinks, find_duplicate_groups};
use crate::export::escape_json;
use crate::junk::{clean_junk_items, JunkItem};
use crate::scanner::run_scan;
use crate::types::{Args, Colors};
use crate::ui::{bytes_to_readable_size, format_proportion_bar, get_drive_space};

pub fn open_app_window(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let edge_res = std::process::Command::new("msedge")
            .args([
                format!("--app={}", url),
                "--window-size=1100,820".to_string(),
            ])
            .spawn();
        if edge_res.is_err() {
            let chrome_res = std::process::Command::new("chrome")
                .args([
                    format!("--app={}", url),
                    "--window-size=1100,820".to_string(),
                ])
                .spawn();
            if chrome_res.is_err() {
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", url])
                    .spawn();
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let chrome_res = std::process::Command::new(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        )
        .args([
            format!("--app={}", url),
            "--window-size=1100,820".to_string(),
        ])
        .spawn();
        if chrome_res.is_err() {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let candidates = [
            "google-chrome",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "microsoft-edge",
        ];
        let mut launched = false;
        for bin in candidates {
            if std::process::Command::new(bin)
                .args([
                    format!("--app={}", url),
                    "--window-size=1100,820".to_string(),
                ])
                .spawn()
                .is_ok()
            {
                launched = true;
                break;
            }
        }
        if !launched {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
    }
}

pub fn start_gui_server(default_args: Args) {
    let listener = match TcpListener::bind("127.0.0.1:8765") {
        Ok(l) => l,
        Err(_) => match TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind local UI window server: {}", e);
                return;
            }
        },
    };

    let port = listener.local_addr().map(|a| a.port()).unwrap_or(8765);
    let url = format!("http://127.0.0.1:{}", port);
    println!("\nOpening standalone UI window at {}", url);
    println!("Press Ctrl+C to close.\n");
    open_app_window(&url);

    let shared_args = Arc::new(default_args);

    for stream in listener.incoming().flatten() {
        let args = Arc::clone(&shared_args);
        thread::spawn(move || {
            handle_client(stream, &args);
        });
    }
}

fn handle_client(mut stream: TcpStream, default_args: &Args) {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    let mut content_length = 0;
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).is_err() || header_line.trim().is_empty() {
            break;
        }
        if header_line.to_lowercase().starts_with("content-length:") {
            if let Some(val) = header_line.split(':').nth(1) {
                content_length = val.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body);
    }

    if method == "GET" && (path == "/" || path == "/index.html") {
        let html = get_html_content(default_args);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = stream.write_all(response.as_bytes());
    } else if method == "POST" && path == "/api/scan" {
        let body_str = String::from_utf8_lossy(&body);
        let json_resp = handle_scan_request(&body_str, default_args);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json_resp.len(),
            json_resp
        );
        let _ = stream.write_all(response.as_bytes());
    } else if method == "POST" && path == "/api/delete" {
        let body_str = String::from_utf8_lossy(&body);
        let json_resp = handle_delete_request(&body_str);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json_resp.len(),
            json_resp
        );
        let _ = stream.write_all(response.as_bytes());
    } else if method == "POST" && path == "/api/clean-junk" {
        let body_str = String::from_utf8_lossy(&body);
        let json_resp = handle_clean_junk_request(&body_str);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json_resp.len(),
            json_resp
        );
        let _ = stream.write_all(response.as_bytes());
    } else if method == "POST" && path == "/api/dedupe" {
        let body_str = String::from_utf8_lossy(&body);
        let json_resp = handle_dedupe_request(&body_str);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json_resp.len(),
            json_resp
        );
        let _ = stream.write_all(response.as_bytes());
    } else {
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(not_found.as_bytes());
    }
}

fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let key = format!("\"{}\":", field);
    if let Some(pos) = json.find(&key) {
        let rest = json[pos + key.len()..].trim_start();
        if let Some(inner) = rest.strip_prefix('"') {
            if let Some(end) = inner.find('"') {
                return Some(inner[..end].to_string());
            }
        } else {
            let end = rest.find([',', '}', ']']).unwrap_or(rest.len());
            return Some(rest[..end].trim().to_string());
        }
    }
    None
}

fn handle_scan_request(body: &str, default_args: &Args) -> String {
    let drives_val = extract_json_field(body, "drives").unwrap_or_else(|| ".".to_string());
    let top_val = extract_json_field(body, "top")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default_args.top);
    let max_depth_val = extract_json_field(body, "max_depth").and_then(|s| s.parse::<usize>().ok());
    let filter_val = extract_json_field(body, "filter").filter(|s| !s.is_empty());
    let find_dups_val = extract_json_field(body, "find_duplicates")
        .map(|s| s == "true")
        .unwrap_or(false);
    let find_junk_val = extract_json_field(body, "find_junk")
        .map(|s| s == "true")
        .unwrap_or(false);

    let drive_paths: Vec<PathBuf> = drives_val
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();

    let valid_drives: Vec<PathBuf> = if drive_paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        drive_paths
    };

    let mut scan_args = default_args.clone();
    scan_args.top = top_val;
    scan_args.max_depth = max_depth_val;
    scan_args.filter_pattern = filter_val;
    scan_args.find_duplicates = find_dups_val;
    scan_args.find_junk = find_junk_val;
    scan_args.quiet = true;

    let colors = Colors::new(false);
    let results = run_scan(&scan_args, &valid_drives, &colors, false);

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"total_scanned\": {},\n  \"total_elapsed\": {:.2},\n",
        results.total_scanned, results.total_elapsed
    ));

    out.push_str("  \"drives\": [\n");
    for (i, d) in valid_drives.iter().enumerate() {
        let (used, total) = get_drive_space(d).unwrap_or((0, 0));
        let free = total.saturating_sub(used);
        let comma = if i + 1 == valid_drives.len() { "" } else { "," };
        out.push_str(&format!(
            "    {{ \"path\": \"{}\", \"used\": \"{}\", \"total\": \"{}\", \"free\": \"{}\" }}{}\n",
            escape_json(&d.to_string_lossy()),
            bytes_to_readable_size(used),
            bytes_to_readable_size(total),
            bytes_to_readable_size(free),
            comma
        ));
    }
    out.push_str("  ],\n");

    out.push_str("  \"largest_files\": [\n");
    let max_file = results.largest_files.first().map(|f| f.0).unwrap_or(1);
    for (i, (bytes, path)) in results.largest_files.iter().enumerate() {
        let comma = if i + 1 == results.largest_files.len() {
            ""
        } else {
            ","
        };
        out.push_str(&format!(
            "    {{ \"bytes\": {}, \"readable\": \"{}\", \"bar\": \"{}\", \"path\": \"{}\" }}{}\n",
            bytes,
            bytes_to_readable_size(*bytes),
            format_proportion_bar(*bytes, max_file),
            escape_json(&path.to_string_lossy()),
            comma
        ));
    }
    out.push_str("  ],\n");

    out.push_str("  \"largest_folders\": [\n");
    let max_folder = results.largest_folders.first().map(|f| f.0).unwrap_or(1);
    for (i, (bytes, path)) in results.largest_folders.iter().enumerate() {
        let comma = if i + 1 == results.largest_folders.len() {
            ""
        } else {
            ","
        };
        out.push_str(&format!(
            "    {{ \"bytes\": {}, \"readable\": \"{}\", \"bar\": \"{}\", \"path\": \"{}\" }}{}\n",
            bytes,
            bytes_to_readable_size(*bytes),
            format_proportion_bar(*bytes, max_folder),
            escape_json(&path.to_string_lossy()),
            comma
        ));
    }
    out.push_str("  ],\n");

    out.push_str("  \"size_histogram\": [\n");
    for (i, (bucket, bytes, count)) in results.size_histogram.iter().enumerate() {
        let comma = if i + 1 == results.size_histogram.len() {
            ""
        } else {
            ","
        };
        out.push_str(&format!(
            "    {{ \"bucket\": \"{}\", \"readable\": \"{}\", \"count\": {} }}{}\n",
            escape_json(bucket),
            bytes_to_readable_size(*bytes),
            count,
            comma
        ));
    }
    out.push_str("  ],\n");

    out.push_str("  \"junk_items\": [\n");
    for (i, item) in results.junk_items.iter().enumerate() {
        let comma = if i + 1 == results.junk_items.len() {
            ""
        } else {
            ","
        };
        out.push_str(&format!(
            "    {{ \"category\": \"{}\", \"readable\": \"{}\", \"path\": \"{}\" }}{}\n",
            escape_json(item.category),
            bytes_to_readable_size(item.bytes),
            escape_json(&item.path.to_string_lossy()),
            comma
        ));
    }
    out.push_str("  ],\n");

    out.push_str("  \"duplicates\": [\n");
    if find_dups_val {
        let dups = find_duplicate_groups(results.size_candidates, top_val);
        for (i, (wasted, paths)) in dups.iter().enumerate() {
            let comma = if i + 1 == dups.len() { "" } else { "," };
            let paths_json: Vec<String> = paths
                .iter()
                .map(|p| format!("\"{}\"", escape_json(&p.to_string_lossy())))
                .collect();
            out.push_str(&format!(
                "    {{ \"wasted\": \"{}\", \"paths\": [{}] }}{}\n",
                bytes_to_readable_size(*wasted),
                paths_json.join(", "),
                comma
            ));
        }
    }
    out.push_str("  ]\n");

    out.push('}');
    out
}

fn handle_delete_request(body: &str) -> String {
    let mut deleted = 0;
    if let Some(pos) = body.find("\"paths\":") {
        let rest = &body[pos + 8..];
        for part in rest.split('"') {
            let trimmed = part.trim();
            if trimmed.starts_with('/')
                || trimmed.starts_with('.')
                || (trimmed.len() > 2 && trimmed.chars().nth(1) == Some(':'))
            {
                let p = PathBuf::from(trimmed);
                let ok = if p.is_file() {
                    std::fs::remove_file(&p).is_ok()
                } else if p.is_dir() {
                    std::fs::remove_dir_all(&p).is_ok()
                } else {
                    false
                };
                if ok {
                    deleted += 1;
                }
            }
        }
    }
    format!("{{ \"deleted\": {} }}", deleted)
}

fn handle_clean_junk_request(body: &str) -> String {
    let mut deleted = 0;
    let mut items = Vec::new();
    if let Some(pos) = body.find("\"paths\":") {
        let rest = &body[pos + 8..];
        for part in rest.split('"') {
            let trimmed = part.trim();
            if trimmed.starts_with('/')
                || trimmed.starts_with('.')
                || (trimmed.len() > 2 && trimmed.chars().nth(1) == Some(':'))
            {
                items.push(JunkItem {
                    path: PathBuf::from(trimmed),
                    category: "Junk",
                    bytes: 0,
                });
            }
        }
    }
    let (d, reclaimed) = clean_junk_items(&items);
    deleted += d;
    format!(
        "{{ \"deleted\": {}, \"reclaimed\": \"{}\" }}",
        deleted,
        bytes_to_readable_size(reclaimed)
    )
}

fn handle_dedupe_request(body: &str) -> String {
    let mut groups = Vec::new();
    if let Some(pos) = body.find("\"paths\":") {
        let rest = &body[pos + 8..];
        let mut group_paths = Vec::new();
        for part in rest.split('"') {
            let trimmed = part.trim();
            if trimmed.starts_with('/')
                || trimmed.starts_with('.')
                || (trimmed.len() > 2 && trimmed.chars().nth(1) == Some(':'))
            {
                group_paths.push(PathBuf::from(trimmed));
            }
        }
        if group_paths.len() >= 2 {
            let size = std::fs::metadata(&group_paths[0])
                .map(|m| m.len())
                .unwrap_or(0);
            groups.push((size * (group_paths.len() as u64 - 1), group_paths));
        }
    }
    let (replaced, reclaimed) = deduplicate_with_hardlinks(&groups);
    format!(
        "{{ \"replaced\": {}, \"reclaimed\": \"{}\" }}",
        replaced,
        bytes_to_readable_size(reclaimed)
    )
}

fn get_html_content(default_args: &Args) -> String {
    let default_drives = default_args.drives.join(", ");
    let initial_drive = default_args
        .drives
        .first()
        .map(|s| s.as_str())
        .unwrap_or(".");
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Disk Usage Scanner</title>
<style>
  * {{ box-sizing: border-box; }}
  body {{
    background-color: #000000;
    color: #00ff66;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    margin: 0;
    padding: 24px;
    line-height: 1.4;
  }}
  h1, h2, h3 {{
    color: #00ffff;
    margin-top: 0;
    font-weight: 600;
    letter-spacing: 0.5px;
  }}
  .header {{
    border-bottom: 2px solid #222222;
    padding-bottom: 12px;
    margin-bottom: 20px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }}
  .header-tag {{
    color: #888888;
    font-size: 13px;
  }}
  .mode-selector {{
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    margin-bottom: 20px;
  }}
  .mode-card {{
    background: #080808;
    border: 1px solid #333333;
    padding: 20px;
    cursor: pointer;
    transition: all 0.15s ease-in-out;
    text-align: left;
  }}
  .mode-card:hover {{
    border-color: #00ff66;
    background: #0f180f;
  }}
  .mode-card.primary {{
    border-color: #00ff66;
  }}
  .mode-title {{
    font-size: 16px;
    font-weight: bold;
    color: #ffffff;
    margin-bottom: 6px;
  }}
  .mode-desc {{
    font-size: 13px;
    color: #888888;
  }}
  .panel {{
    border: 1px solid #222222;
    padding: 16px;
    margin-bottom: 20px;
    background: #080808;
  }}
  .controls-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
    margin-bottom: 16px;
  }}
  label {{
    display: block;
    font-size: 12px;
    color: #888888;
    margin-bottom: 4px;
    text-transform: uppercase;
  }}
  input[type="text"], input[type="number"] {{
    width: 100%;
    background: #000000;
    border: 1px solid #333333;
    color: #ffffff;
    font-family: inherit;
    font-size: 14px;
    padding: 8px 10px;
    outline: none;
  }}
  input[type="text"]:focus, input[type="number"]:focus {{
    border-color: #00ff66;
  }}
  .checkbox-label {{
    display: flex;
    align-items: center;
    gap: 8px;
    color: #cccccc;
    font-size: 13px;
    cursor: pointer;
    margin-top: 8px;
    text-transform: none;
  }}
  button {{
    background: #000000;
    color: #00ff66;
    border: 1px solid #00ff66;
    font-family: inherit;
    font-size: 13px;
    font-weight: bold;
    padding: 10px 20px;
    cursor: pointer;
    text-transform: uppercase;
    transition: background 0.15s, color 0.15s;
  }}
  button:hover {{
    background: #00ff66;
    color: #000000;
  }}
  button.danger {{
    color: #ff4444;
    border-color: #ff4444;
  }}
  button.danger:hover {{
    background: #ff4444;
    color: #000000;
  }}
  table {{
    width: 100%;
    border-collapse: collapse;
    margin-top: 10px;
    font-size: 13px;
  }}
  th, td {{
    text-align: left;
    padding: 8px;
    border-bottom: 1px solid #1a1a1a;
  }}
  th {{
    color: #ffff00;
    border-bottom: 1px solid #333333;
    text-transform: uppercase;
    font-size: 12px;
  }}
  tr:hover td {{
    background: #111111;
  }}
  .path-cell {{
    word-break: break-all;
    color: #dddddd;
  }}
  .size-cell {{
    color: #00ff66;
    white-space: nowrap;
    width: 110px;
  }}
  .bar-cell {{
    color: #555555;
    font-family: inherit;
    white-space: nowrap;
    width: 120px;
  }}
  .badge {{
    background: #181818;
    color: #00ffff;
    padding: 2px 6px;
    font-size: 11px;
    border: 1px solid #282828;
  }}
  #status-bar {{
    padding: 10px 14px;
    background: #111111;
    border-left: 4px solid #00ff66;
    margin-bottom: 16px;
    color: #aaaaaa;
    font-size: 13px;
  }}
</style>
</head>
<body>
  <div class="header">
    <h1>[ DISK USAGE SCANNER ]</h1>
    <div class="header-tag">STANDALONE CONTROLLER</div>
  </div>

  <div id="status-bar">Select Default Scan for instant one-click analysis, or Advance Option to configure parameters.</div>

  <div class="mode-selector">
    <div class="mode-card primary" onclick="runDefaultScan()">
      <div class="mode-title">[ 1. DEFAULT SCAN ]</div>
      <div class="mode-desc">Instantly scan system drives ({}) with recommended settings</div>
    </div>
    <div class="mode-card" onclick="toggleAdvancedOptions()">
      <div class="mode-title">[ 2. ADVANCE OPTION ]</div>
      <div class="mode-desc">Configure custom drives, filters, depth limits, junk cleaner, and duplicates</div>
    </div>
  </div>

  <div class="panel" id="advance-panel" style="display: none;">
    <h2>Advance Configuration</h2>
    <div class="controls-grid">
      <div>
        <label>Target Drive / Path</label>
        <input type="text" id="target-drives" value="{}">
      </div>
      <div>
        <label>Top Results Count</label>
        <input type="number" id="top-n" value="20">
      </div>
      <div>
        <label>Max Folder Depth</label>
        <input type="number" id="max-depth" placeholder="Unlimited">
      </div>
      <div>
        <label>Filter Pattern (e.g. *.iso)</label>
        <input type="text" id="filter-pattern" placeholder="Optional">
      </div>
    </div>
    <div style="display: flex; gap: 24px; flex-wrap: wrap; margin-bottom: 16px;">
      <label class="checkbox-label">
        <input type="checkbox" id="chk-dups" checked> Find Duplicates
      </label>
      <label class="checkbox-label">
        <input type="checkbox" id="chk-junk" checked> Scan Junk & Caches
      </label>
    </div>
    <div style="display: flex; gap: 12px;">
      <button onclick="runCustomScan()">Execute Custom Scan</button>
      <button onclick="quickTarget('.')">Target Current Dir</button>
      <button onclick="quickTarget('/')">Target Root (/)</button>
    </div>
  </div>

  <div id="results-area" style="display: none;">
    <div class="panel" id="capacity-panel">
      <h2>Drive Capacity</h2>
      <div id="capacity-content"></div>
    </div>

    <div class="panel">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <h2>Largest Files</h2>
        <button class="danger" onclick="deleteSelectedFiles()">Delete Selected</button>
      </div>
      <table id="files-table">
        <thead>
          <tr>
            <th style="width: 40px;"><input type="checkbox" onclick="toggleAllFiles(this)"></th>
            <th>Size</th>
            <th>Proportion</th>
            <th>Path</th>
          </tr>
        </thead>
        <tbody id="files-tbody"></tbody>
      </table>
    </div>

    <div class="panel">
      <h2>Largest Folders</h2>
      <table id="folders-table">
        <thead>
          <tr>
            <th>Size</th>
            <th>Proportion</th>
            <th>Folder Path</th>
          </tr>
        </thead>
        <tbody id="folders-tbody"></tbody>
      </table>
    </div>

    <div class="panel" id="junk-panel" style="display: none;">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <h2>Detected Junk & Build Caches</h2>
        <button class="danger" onclick="cleanSelectedJunk()">Clean Selected Junk</button>
      </div>
      <table id="junk-table">
        <thead>
          <tr>
            <th style="width: 40px;"><input type="checkbox" onclick="toggleAllJunk(this)"></th>
            <th>Category</th>
            <th>Path</th>
          </tr>
        </thead>
        <tbody id="junk-tbody"></tbody>
      </table>
    </div>

    <div class="panel" id="hist-panel">
      <h2>File Size Distribution Histogram</h2>
      <table id="hist-table">
        <thead>
          <tr>
            <th>Bucket</th>
            <th>Total Size</th>
            <th>File Count</th>
          </tr>
        </thead>
        <tbody id="hist-tbody"></tbody>
      </table>
    </div>

    <div class="panel" id="dups-panel" style="display: none;">
      <h2>Duplicate Files</h2>
      <div id="dups-content"></div>
    </div>
  </div>

<script>
  function quickTarget(t) {{
    document.getElementById('target-drives').value = t;
  }}

  function toggleAdvancedOptions() {{
    const p = document.getElementById('advance-panel');
    p.style.display = p.style.display === 'none' ? 'block' : 'none';
  }}

  function toggleAllFiles(master) {{
    document.querySelectorAll('.file-chk').forEach(c => c.checked = master.checked);
  }}

  function toggleAllJunk(master) {{
    document.querySelectorAll('.junk-chk').forEach(c => c.checked = master.checked);
  }}

  function runDefaultScan() {{
    document.getElementById('target-drives').value = "{}";
    document.getElementById('top-n').value = 20;
    document.getElementById('max-depth').value = "";
    document.getElementById('filter-pattern').value = "";
    document.getElementById('chk-dups').checked = true;
    document.getElementById('chk-junk').checked = true;
    executeScan({{
      drives: "{}",
      top: 20,
      max_depth: null,
      filter: "",
      find_duplicates: true,
      find_junk: true
    }});
  }}

  function runCustomScan() {{
    executeScan({{
      drives: document.getElementById('target-drives').value,
      top: parseInt(document.getElementById('top-n').value) || 20,
      max_depth: parseInt(document.getElementById('max-depth').value) || null,
      filter: document.getElementById('filter-pattern').value.trim(),
      find_duplicates: document.getElementById('chk-dups').checked,
      find_junk: document.getElementById('chk-junk').checked
    }});
  }}

  async function executeScan(payload) {{
    const statusBar = document.getElementById('status-bar');
    statusBar.innerText = 'Scanning disk in progress... please wait.';
    statusBar.style.color = '#ffff00';

    try {{
      const res = await fetch('/api/scan', {{
        method: 'POST',
        headers: {{ 'Content-Type': 'application/json' }},
        body: JSON.stringify(payload)
      }});
      const data = await res.json();
      renderResults(data);
      statusBar.innerText = `Scan complete: ${{data.total_scanned}} files scanned in ${{data.total_elapsed}}s.`;
      statusBar.style.color = '#00ff66';
    }} catch (e) {{
      statusBar.innerText = 'Scan failed: ' + e;
      statusBar.style.color = '#ff4444';
    }}
  }}

  function renderResults(data) {{
    document.getElementById('results-area').style.display = 'block';

    const capContent = document.getElementById('capacity-content');
    capContent.innerHTML = data.drives.map(d =>
      `<div><strong>${{d.path}}</strong>: ${{d.used}} used / ${{d.total}} total (${{d.free}} free)</div>`
    ).join('');

    const filesTbody = document.getElementById('files-tbody');
    filesTbody.innerHTML = data.largest_files.map(f =>
      `<tr>
        <td><input type="checkbox" class="file-chk" value="${{f.path}}"></td>
        <td class="size-cell">${{f.readable}}</td>
        <td class="bar-cell">${{f.bar}}</td>
        <td class="path-cell">${{f.path}}</td>
      </tr>`
    ).join('');

    const foldersTbody = document.getElementById('folders-tbody');
    foldersTbody.innerHTML = data.largest_folders.map(f =>
      `<tr>
        <td class="size-cell">${{f.readable}}</td>
        <td class="bar-cell">${{f.bar}}</td>
        <td class="path-cell">${{f.path}}</td>
      </tr>`
    ).join('');

    const histTbody = document.getElementById('hist-tbody');
    histTbody.innerHTML = data.size_histogram.map(h =>
      `<tr>
        <td style="color: #ffff00;">${{h.bucket}}</td>
        <td class="size-cell">${{h.readable}}</td>
        <td>${{h.count.toLocaleString()}}</td>
      </tr>`
    ).join('');

    const junkPanel = document.getElementById('junk-panel');
    if (data.junk_items && data.junk_items.length > 0) {{
      junkPanel.style.display = 'block';
      const junkTbody = document.getElementById('junk-tbody');
      junkTbody.innerHTML = data.junk_items.map(j =>
        `<tr>
          <td><input type="checkbox" class="junk-chk" value="${{j.path}}"></td>
          <td><span class="badge">${{j.category}}</span></td>
          <td class="path-cell">${{j.path}}</td>
        </tr>`
      ).join('');
    }} else {{
      junkPanel.style.display = 'none';
    }}

    const dupsPanel = document.getElementById('dups-panel');
    if (data.duplicates && data.duplicates.length > 0) {{
      dupsPanel.style.display = 'block';
      const dupsContent = document.getElementById('dups-content');
      dupsContent.innerHTML = data.duplicates.map((g, idx) =>
        `<div style="margin-bottom: 12px; padding: 8px; border: 1px solid #1a1a1a;">
          <div style="color: #ff4444; font-weight: bold; margin-bottom: 4px;">
            Wasted Space: ${{g.wasted}} (${{g.paths.length}} copies)
          </div>
          ${{g.paths.map(p => `<div class="path-cell" style="padding-left: 12px;">- ${{p}}</div>`).join('')}}
        </div>`
      ).join('');
    }} else {{
      dupsPanel.style.display = 'none';
    }}
  }}

  async function deleteSelectedFiles() {{
    const chks = document.querySelectorAll('.file-chk:checked');
    if (chks.length === 0) return alert('No files selected.');
    if (!confirm(`Are you sure you want to permanently delete ${{chks.length}} selected file(s)?`)) return;

    const paths = Array.from(chks).map(c => c.value);
    const res = await fetch('/api/delete', {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{ paths }})
    }});
    const data = await res.json();
    alert(`Deleted ${{data.deleted}} file(s).`);
    runDefaultScan();
  }}

  async function cleanSelectedJunk() {{
    const chks = document.querySelectorAll('.junk-chk:checked');
    if (chks.length === 0) return alert('No junk items selected.');
    if (!confirm(`Permanently remove ${{chks.length}} selected junk folder(s)?`)) return;

    const paths = Array.from(chks).map(c => c.value);
    const res = await fetch('/api/clean-junk', {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{ paths }})
    }});
    const data = await res.json();
    alert(`Cleaned ${{data.deleted}} junk items, reclaimed ${{data.reclaimed}}.`);
    runDefaultScan();
  }}
</script>
</body>
</html>
"##,
        default_drives, initial_drive, initial_drive, initial_drive
    )
}
