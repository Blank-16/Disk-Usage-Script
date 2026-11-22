use std::io::Write;
use std::path::PathBuf;

use crate::ui::bytes_to_readable_size;

pub fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn export_json(
    filename: &str,
    total_files: u64,
    elapsed_seconds: f64,
    largest_files: &[(u64, PathBuf)],
    largest_folders: &[(u64, PathBuf)],
    top_extensions: &[(String, u64, u64)],
    age_dist: &[(String, u64, u64)],
    duplicates: &[(u64, Vec<PathBuf>)],
) -> std::io::Result<()> {
    let mut f = std::fs::File::create(filename)?;
    writeln!(f, "{{")?;
    writeln!(f, "  \"summary\": {{")?;
    writeln!(f, "    \"total_files_scanned\": {},", total_files)?;
    writeln!(f, "    \"elapsed_seconds\": {:.2}", elapsed_seconds)?;
    writeln!(f, "  }},")?;

    writeln!(f, "  \"largest_files\": [")?;
    for (i, (size, path)) in largest_files.iter().enumerate() {
        let comma = if i + 1 == largest_files.len() {
            ""
        } else {
            ","
        };
        writeln!(
            f,
            "    {{ \"path\": \"{}\", \"bytes\": {} }}{}",
            escape_json(&path.to_string_lossy()),
            size,
            comma
        )?;
    }
    writeln!(f, "  ],")?;

    writeln!(f, "  \"largest_folders\": [")?;
    for (i, (size, path)) in largest_folders.iter().enumerate() {
        let comma = if i + 1 == largest_folders.len() {
            ""
        } else {
            ","
        };
        writeln!(
            f,
            "    {{ \"path\": \"{}\", \"bytes\": {} }}{}",
            escape_json(&path.to_string_lossy()),
            size,
            comma
        )?;
    }
    writeln!(f, "  ],")?;

    writeln!(f, "  \"top_extensions\": [")?;
    for (i, (ext, size, count)) in top_extensions.iter().enumerate() {
        let comma = if i + 1 == top_extensions.len() {
            ""
        } else {
            ","
        };
        writeln!(
            f,
            "    {{ \"extension\": \"{}\", \"bytes\": {}, \"count\": {} }}{}",
            escape_json(ext),
            size,
            count,
            comma
        )?;
    }
    writeln!(f, "  ],")?;

    writeln!(f, "  \"age_distribution\": [")?;
    for (i, (label, size, count)) in age_dist.iter().enumerate() {
        let comma = if i + 1 == age_dist.len() { "" } else { "," };
        writeln!(
            f,
            "    {{ \"label\": \"{}\", \"bytes\": {}, \"count\": {} }}{}",
            escape_json(label),
            size,
            count,
            comma
        )?;
    }
    writeln!(f, "  ],")?;

    writeln!(f, "  \"duplicates\": [")?;
    for (i, (wasted, paths)) in duplicates.iter().enumerate() {
        let comma = if i + 1 == duplicates.len() { "" } else { "," };
        let paths_json: Vec<String> = paths
            .iter()
            .map(|p| format!("\"{}\"", escape_json(&p.to_string_lossy())))
            .collect();
        writeln!(
            f,
            "    {{ \"wasted_bytes\": {}, \"paths\": [{}] }}{}",
            wasted,
            paths_json.join(", "),
            comma
        )?;
    }
    writeln!(f, "  ]")?;
    writeln!(f, "}}")?;
    Ok(())
}

pub fn export_csv(
    filename: &str,
    largest_files: &[(u64, PathBuf)],
    largest_folders: &[(u64, PathBuf)],
    top_extensions: &[(String, u64, u64)],
    age_dist: &[(String, u64, u64)],
) -> std::io::Result<()> {
    let mut f = std::fs::File::create(filename)?;
    writeln!(f, "category,path_or_label,bytes,readable_size,extra_count")?;

    for (size, path) in largest_files {
        writeln!(
            f,
            "file,\"{}\",{},\"{}\",",
            path.display(),
            size,
            bytes_to_readable_size(*size)
        )?;
    }

    for (size, path) in largest_folders {
        writeln!(
            f,
            "folder,\"{}\",{},\"{}\",",
            path.display(),
            size,
            bytes_to_readable_size(*size)
        )?;
    }

    for (ext, size, count) in top_extensions {
        writeln!(
            f,
            "extension,\"{}\",{},\"{}\",{}",
            ext,
            size,
            bytes_to_readable_size(*size),
            count
        )?;
    }

    for (label, size, count) in age_dist {
        writeln!(
            f,
            "age_group,\"{}\",{},\"{}\",{}",
            label,
            size,
            bytes_to_readable_size(*size),
            count
        )?;
    }

    Ok(())
}

pub fn export_html_report(
    filename: &str,
    total_files: u64,
    elapsed_seconds: f64,
    largest_files: &[(u64, PathBuf)],
    largest_folders: &[(u64, PathBuf)],
    top_extensions: &[(String, u64, u64)],
    age_dist: &[(String, u64, u64)],
) -> std::io::Result<()> {
    let mut f = std::fs::File::create(filename)?;

    let mut folders_json = String::new();
    for (i, (size, path)) in largest_folders.iter().enumerate() {
        let comma = if i + 1 == largest_folders.len() {
            ""
        } else {
            ","
        };
        folders_json.push_str(&format!(
            "{{ name: \"{}\", size: {}, readable: \"{}\" }}{}",
            escape_json(&path.to_string_lossy()),
            size,
            bytes_to_readable_size(*size),
            comma
        ));
    }

    let mut files_json = String::new();
    for (i, (size, path)) in largest_files.iter().enumerate() {
        let comma = if i + 1 == largest_files.len() {
            ""
        } else {
            ","
        };
        files_json.push_str(&format!(
            "{{ name: \"{}\", size: {}, readable: \"{}\" }}{}",
            escape_json(&path.to_string_lossy()),
            size,
            bytes_to_readable_size(*size),
            comma
        ));
    }

    let mut ext_json = String::new();
    for (i, (ext, size, count)) in top_extensions.iter().enumerate() {
        let comma = if i + 1 == top_extensions.len() {
            ""
        } else {
            ","
        };
        ext_json.push_str(&format!(
            "{{ ext: \"{}\", size: {}, readable: \"{}\", count: {} }}{}",
            escape_json(ext),
            size,
            bytes_to_readable_size(*size),
            count,
            comma
        ));
    }

    let mut age_json = String::new();
    for (i, (label, size, count)) in age_dist.iter().enumerate() {
        let comma = if i + 1 == age_dist.len() { "" } else { "," };
        age_json.push_str(&format!(
            "{{ label: \"{}\", size: {}, readable: \"{}\", count: {} }}{}",
            escape_json(label),
            size,
            bytes_to_readable_size(*size),
            count,
            comma
        ));
    }

    write!(
        f,
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Disk Usage Scanner Report</title>
<style>
* {{ box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; background: #0b0f19; color: #f1f5f9; margin: 0; padding: 24px; }}
h1 {{ font-size: 26px; color: #38bdf8; margin: 0 0 8px 0; }}
.summary {{ font-size: 14px; color: #94a3b8; margin-bottom: 24px; }}
.card {{ background: #131c2e; border: 1px solid #1e293b; border-radius: 10px; padding: 20px; margin-bottom: 24px; box-shadow: 0 4px 6px -1px rgba(0,0,0,0.3); }}
.card-title {{ font-size: 16px; font-weight: 600; color: #f8fafc; margin-bottom: 16px; display: flex; justify-content: space-between; }}
#treemap-container {{ position: relative; width: 100%; height: 380px; background: #080d1a; border-radius: 8px; overflow: hidden; }}
canvas {{ width: 100%; height: 100%; display: block; }}
.tooltip {{ position: absolute; display: none; background: #1e293b; color: #fff; padding: 8px 12px; border-radius: 6px; font-size: 12px; pointer-events: none; border: 1px solid #475569; z-index: 100; }}
.grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }}
@media (max-width: 900px) {{ .grid {{ grid-template-columns: 1fr; }} }}
table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
th, td {{ text-align: left; padding: 8px 10px; border-bottom: 1px solid #1e293b; }}
th {{ color: #64748b; font-weight: 600; text-transform: uppercase; font-size: 11px; }}
tr:hover {{ background: #1c273c; }}
.size {{ color: #4ade80; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-weight: 600; width: 100px; }}
.count {{ color: #94a3b8; width: 70px; }}
.bar-col {{ width: 100px; }}
.bar-track {{ background: #1e293b; height: 6px; border-radius: 3px; overflow: hidden; }}
.bar-fill {{ background: #38bdf8; height: 100%; }}
.path {{ word-break: break-all; color: #e2e8f0; }}
</style>
</head>
<body>
<h1>Disk Usage Scanner Report</h1>
<div class="summary">Scanned {files_scanned} files in {elapsed:.2}s</div>

<div class="card">
  <div class="card-title">Interactive Visual Treemap (Top Folders)</div>
  <div id="treemap-container">
    <canvas id="treemapCanvas"></canvas>
    <div id="tooltip" class="tooltip"></div>
  </div>
</div>

<div class="grid">
  <div class="card">
    <div class="card-title">Top Largest Files</div>
    <table id="filesTable">
      <thead><tr><th>Size</th><th class="bar-col">Share</th><th>Path</th></tr></thead>
      <tbody></tbody>
    </table>
  </div>

  <div class="card">
    <div class="card-title">Top Largest Folders</div>
    <table id="foldersTable">
      <thead><tr><th>Size</th><th class="bar-col">Share</th><th>Folder</th></tr></thead>
      <tbody></tbody>
    </table>
  </div>
</div>

<div class="grid">
  <div class="card">
    <div class="card-title">File Types by Disk Usage</div>
    <table id="extTable">
      <thead><tr><th>Size</th><th class="count">Count</th><th>Extension</th></tr></thead>
      <tbody></tbody>
    </table>
  </div>

  <div class="card">
    <div class="card-title">File Age Distribution</div>
    <table id="ageTable">
      <thead><tr><th>Size</th><th class="count">Files</th><th>Age Bucket</th></tr></thead>
      <tbody></tbody>
    </table>
  </div>
</div>

<script>
const folders = [{folders}];
const files = [{files}];
const extensions = [{exts}];
const ageGroups = [{ages}];

function renderTable(tableId, data, isExt) {{
  const tbody = document.querySelector("#" + tableId + " tbody");
  if (!data.length) {{ tbody.innerHTML = "<tr><td colspan='3'>No items</td></tr>"; return; }}
  const max = data[0].size || 1;
  data.forEach(item => {{
    const pct = Math.round((item.size / max) * 100);
    const tr = document.createElement("tr");
    if (isExt) {{
      tr.innerHTML = `<td class="size">${{item.readable}}</td><td class="count">${{item.count.toLocaleString()}}</td><td>${{item.ext || item.label}}</td>`;
    }} else {{
      tr.innerHTML = `<td class="size">${{item.readable}}</td><td class="bar-col"><div class="bar-track"><div class="bar-fill" style="width:${{pct}}%"></div></div></td><td class="path">${{item.name}}</td>`;
    }}
    tbody.appendChild(tr);
  }});
}}

renderTable("filesTable", files, false);
renderTable("foldersTable", folders, false);
renderTable("extTable", extensions, true);
renderTable("ageTable", ageGroups, true);

const canvas = document.getElementById("treemapCanvas");
const ctx = canvas.getContext("2d");
const container = document.getElementById("treemap-container");
const tooltip = document.getElementById("tooltip");

function resizeCanvas() {{
  canvas.width = container.clientWidth * window.devicePixelRatio;
  canvas.height = container.clientHeight * window.devicePixelRatio;
  ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
  drawTreemap();
}}

let rects = [];
const colors = ["#0284c7", "#0d9488", "#4f46e5", "#b45309", "#059669", "#7c3aed", "#be185d", "#c2410c"];

function drawTreemap() {{
  const w = container.clientWidth;
  const h = container.clientHeight;
  ctx.clearRect(0, 0, w, h);
  rects = [];
  if (!folders.length) return;

  const total = folders.reduce((sum, f) => sum + f.size, 0) || 1;
  let curX = 0;

  folders.forEach((f, idx) => {{
    const ratio = f.size / total;
    const rw = Math.max(2, w * ratio);
    const color = colors[idx % colors.length];
    ctx.fillStyle = color;
    ctx.fillRect(curX, 0, rw - 1, h);

    if (rw > 30) {{
      ctx.fillStyle = "#ffffff";
      ctx.font = "bold 12px sans-serif";
      const label = f.name.split("/").pop().split("\\").pop();
      ctx.fillText(label, curX + 6, 24);
      ctx.font = "11px monospace";
      ctx.fillStyle = "rgba(255,255,255,0.8)";
      ctx.fillText(f.readable, curX + 6, 40);
    }}

    rects.push({{ x: curX, y: 0, w: rw, h: h, item: f }});
    curX += rw;
  }});
}}

window.addEventListener("resize", resizeCanvas);
resizeCanvas();

container.addEventListener("mousemove", e => {{
  const rect = container.getBoundingClientRect();
  const mx = e.clientX - rect.left;
  const my = e.clientY - rect.top;
  const match = rects.find(r => mx >= r.x && mx <= r.x + r.w && my >= r.y && my <= r.y + r.h);
  if (match) {{
    tooltip.style.display = "block";
    tooltip.style.left = (mx + 10) + "px";
    tooltip.style.top = (my + 10) + "px";
    tooltip.innerHTML = `<strong>${{match.item.name}}</strong><br>Size: ${{match.item.readable}}`;
  }} else {{
    tooltip.style.display = "none";
  }}
}});

container.addEventListener("mouseleave", () => {{ tooltip.style.display = "none"; }});
</script>
</body>
</html>
"##,
        files_scanned = total_files,
        elapsed = elapsed_seconds,
        folders = folders_json,
        files = files_json,
        exts = ext_json,
        ages = age_json
    )?;

    Ok(())
}
