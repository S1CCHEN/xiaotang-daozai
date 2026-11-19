#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
把官方岛仔源码打上「四环余额面板」补丁，并把内置更新源指向自己的 fork，
形成自更新闭环。设计成幂等：重复执行不会叠加改动。

用法（在源码根目录执行）：
    DAOZAI_FORK_REPO=YourName/xiaotang-daozai python apply_patch.py <src_root> <patch_dir>

环境变量 DAOZAI_FORK_REPO 指定 fork 的 owner/repo，用于改写 updates.rs 更新源。
不设置则跳过更新源改写（本地调试用）。
"""
import os
import re
import shutil
import sys


def read(path):
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


def write(path, text):
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(text)


def patch_lib(src):
    p = os.path.join(src, "src", "lib.rs")
    t = read(p)
    if re.search(r"^\s*mod\s+quad_dash\s*;", t, re.M):
        print("  lib.rs: 已打补丁，跳过")
        return
    # 在 quota_ui 声明行后插入模块声明（容忍 pub / 顺序差异）
    m = re.search(r"^(\s*)(?:pub\s+)?mod\s+quota_ui\s*;", t, re.M)
    if not m:
        raise RuntimeError("lib.rs: 找不到 mod quota_ui 锚点，源码结构可能已变，请人工检查")
    indent = m.group(1)
    insert_at = m.end()
    t = t[:insert_at] + "\n" + indent + "mod quad_dash;" + t[insert_at:]
    write(p, t)
    print("  lib.rs: 已注册 mod quad_dash")


def patch_app(src):
    p = os.path.join(src, "src", "app.rs")
    t = read(p)
    if "quad_dash::show_quad" in t:
        print("  app.rs: 已打补丁，跳过")
        return
    # 匹配第 6 页路由，容忍缩进与尾随空白
    pat = re.compile(r"^(\s*)6\s*=>\s*self\.page_quota\(c\)\s*,", re.M)
    m = pat.search(t)
    if not m:
        raise RuntimeError("app.rs: 找不到 page_quota 第 6 页路由锚点，源码结构可能已变，请人工检查")
    indent = m.group(1)
    new_line = (f"{indent}6 => {{ if crate::quad_dash::show_quad() "
                f"{{ crate::quad_dash::page(c); }} else "
                f"{{ self.page_quota(c); crate::quad_dash::back_button(c); }} }},")
    t = t[:m.start()] + new_line + t[m.end():]
    write(p, t)
    print("  app.rs: 第 6 页路由已接管为四环总览（可切回原账号视图）")


def patch_updates(src, fork_repo):
    if not fork_repo:
        print("  updates.rs: 未指定 DAOZAI_FORK_REPO，跳过更新源改写")
        return
    if "/" not in fork_repo:
        raise RuntimeError("DAOZAI_FORK_REPO 格式应为 owner/repo，例如 SICHEN/xiaotang-daozai")
    p = os.path.join(src, "src", "updates.rs")
    t = read(p)
    repo = f"https://github.com/{fork_repo}"
    api = f"https://api.github.com/repos/{fork_repo}/releases/latest"
    releases = f"{repo}/releases/latest"
    subs = [
        (r'const\s+REPOSITORY:\s*&str\s*=\s*"[^"]*"\s*;', f'const REPOSITORY: &str = "{repo}";', "REPOSITORY"),
        (r'const\s+API:\s*&str\s*=\s*"[^"]*"\s*;', f'const API: &str = "{api}";', "API"),
        (r'const\s+RELEASES:\s*&str\s*=\s*"[^"]*"\s*;', f'const RELEASES: &str = "{releases}";', "RELEASES"),
    ]
    changed = False
    for pat, repl, name in subs:
        t2, n = re.subn(pat, repl, t, count=1)
        if n == 0:
            raise RuntimeError(f"updates.rs: 找不到 {name} 常量锚点，源码结构可能已变，请人工检查")
        if t2 != t:
            changed = True
        t = t2
    write(p, t)
    print(f"  updates.rs: 更新源已指向 {fork_repo}" if changed else "  updates.rs: 已是目标更新源")


def patch_version_stamp(src):
    """把 app.rc 版本戳同步为 Cargo.toml 的版本，否则 package.ps1 会校验失败。"""
    cargo = read(os.path.join(src, "Cargo.toml"))
    m = re.search(r'(?m)^\s*version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"', cargo)
    if not m:
        raise RuntimeError("Cargo.toml: 找不到版本号")
    ver = m.group(1)
    a, b, c = ver.split(".")
    p = os.path.join(src, "app.rc")
    t = read(p)
    t, n1 = re.subn(r"FILEVERSION\s+[0-9]+,[0-9]+,[0-9]+,[0-9]+", f"FILEVERSION {a},{b},{c},0", t)
    t, n2 = re.subn(r"PRODUCTVERSION\s+[0-9]+,[0-9]+,[0-9]+,[0-9]+", f"PRODUCTVERSION {a},{b},{c},0", t)
    t, n3 = re.subn(r'(VALUE\s+"(?:Product|File)Version",\s*")[0-9]+\.[0-9]+\.[0-9]+(")', rf"\g<1>{ver}\g<2>", t)
    write(p, t)
    print(f"  app.rc: 版本戳已同步为 {ver}（FILE/PRODUCT {n1}/{n2}，字符串 {n3} 处）")


def install_quad(src, patch_dir):
    dst = os.path.join(src, "src", "quad_dash.rs")
    src_file = os.path.join(patch_dir, "quad_dash.rs")
    if not os.path.isfile(src_file):
        raise RuntimeError(f"找不到四环模块源文件：{src_file}")
    shutil.copyfile(src_file, dst)
    print("  src/quad_dash.rs: 已写入")


def main():
    if len(sys.argv) < 3:
        print("用法: apply_patch.py <src_root> <patch_dir>", file=sys.stderr)
        sys.exit(2)
    src, patch_dir = sys.argv[1], sys.argv[2]
    fork_repo = os.environ.get("DAOZAI_FORK_REPO", "").strip()
    print(f"打补丁：src={src}  fork={fork_repo or '(未指定)'}")
    install_quad(src, patch_dir)
    patch_lib(src)
    patch_app(src)
    patch_updates(src, fork_repo)
    patch_version_stamp(src)
    print("补丁完成。")


if __name__ == "__main__":
    main()
