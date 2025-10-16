// Copyright (c) 2018, Gregory Meyer
// Copyright (c) 2025, Fabian Thomys
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
//     * Redistributions of source code must retain the above copyright
//       notice, this list of conditions and the following disclaimer.
//     * Redistributions in binary form must reproduce the above copyright
//       notice, this list of conditions and the following disclaimer in the
//       documentation and/or other materials provided with the distribution.
//     * Neither the name of the <organization> nor the
//       names of its contributors may be used to endorse or promote products
//       derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL <COPYRIGHT HOLDER> BE LIABLE FOR ANY 
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
// (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
// LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
// ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

extern crate clap;
#[macro_use]
extern crate serde_derive;

mod alternative;
mod alternative_db;
mod alternative_list;
mod filesystem;

use alternative::Alternative;
use alternative_db::AlternativeDb;

fn escalate_privileges() -> std::io::Result<()> {
    use std::process::Command;

    let exe = std::env::current_exe()?;
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();

    #[cfg(target_os = "linux")]
    {
        match Command::new("pkexec").arg(&exe).args(&args).status() {
            Ok(status) => {
                let code = status.code().unwrap_or(1);
                std::process::exit(code);
            }
            Err(pkerr) => {
                // Fallback to sudo
                match Command::new("sudo").arg(&exe).args(&args).status() {
                    Ok(status) => {
                        let code = status.code().unwrap_or(1);
                        std::process::exit(code);
                    }
                    Err(_suderr) => {
                        Err(pkerr)
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        match Command::new("sudo").arg(&exe).args(&args).status() {
            Ok(status) => {
                let code = status.code().unwrap_or(1);
                std::process::exit(code);
            }
            Err(e) => Err(e),
        }
    }
}

fn main() {
    let use_gui_flag = std::env::args().any(|a| a == "--gui");
    let euid = nix::unistd::geteuid();
    if !euid.is_root() && !use_gui_flag {
        if let Err(e) = escalate_privileges() {
            eprintln!("update-alternatives: must be run as root (auto-escalation failed: {})", e);
            std::process::exit(1);
        } else {
            unreachable!("escalate_privileges should not return Ok(()) in non-root context");
        }
    }
    
    let matches = app().get_matches();

    let mut db = match read_db("/etc/alternatives") {
        Ok(d) => d,
        Err(_) => std::process::exit(1),
    };

    let use_gui = matches.get_flag("gui");

    let mutated = if use_gui {
        run_gui(&mut db)
    } else {
        match matches.subcommand() {
            Some(("list", sub_m)) => list(&db, sub_m),
            Some(("add", sub_m)) => add(&mut db, sub_m),
            Some(("remove", sub_m)) => remove(&mut db, sub_m),
            Some(("sync", _sub_m)) => sync(&db),
            _ => false,
        }
    };

    if mutated && commit(&db).is_err() {
        std::process::exit(1);
    }
}

fn read_db<P: std::convert::AsRef<std::path::Path>>(path: P)
-> std::io::Result<AlternativeDb> {
    match AlternativeDb::from_folder(path) {
        Ok(d) => {
            println!("update-alternatives: parsed {} alternatives",
                     d.num_alternatives());

            Ok(d)
        },
        Err(e) => {
            eprintln!("update-alternatives: could not read folder \
                      /etc/alternatives: {}", e);

            Err(e)
        }
    }
}

fn list(db: &AlternativeDb, matches: &clap::ArgMatches) -> bool {
    let name = matches
        .get_one::<String>("NAME")
        .or_else(|| matches.get_one::<String>("NAME_POS"))
        .map(|s| s.as_str())
        .unwrap();

    match db.alternatives(name) {
        Some(alternatives) => {
            print!("update-alternatives: {}", alternatives);
        },
        None => {
            eprintln!("update-alternatives: no alternatives found for {}", name);
        }
    }

    false
}

fn add(db: &mut AlternativeDb, matches: &clap::ArgMatches) -> bool {
    let target = matches
        .get_one::<String>("TARGET")
        .or_else(|| matches.get_one::<String>("TARGET_POS"))
        .map(|s| s.as_str())
        .unwrap();
    let name = matches
        .get_one::<String>("NAME")
        .or_else(|| matches.get_one::<String>("NAME_POS"))
        .map(|s| s.as_str())
        .unwrap();
    let weight_str = matches
        .get_one::<String>("WEIGHT")
        .or_else(|| matches.get_one::<String>("WEIGHT_POS"))
        .map(|s| s.as_str())
        .unwrap();

    let weight: i32 = match weight_str.parse() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("update-alternatives: could not parse {} as \
                      weight: {}", weight_str, e);

            std::process::exit(1);
        },
    };

    if db.add_alternative(name, Alternative::from_parts(target, weight)) {
        println!("update-alternatives: added alternative {} for {} with \
                 priority {}", target, name, weight);

        return true;
    }

    false
}

fn remove(db: &mut AlternativeDb, matches: &clap::ArgMatches) -> bool {
    let target = matches
        .get_one::<String>("TARGET")
        .or_else(|| matches.get_one::<String>("TARGET_POS"))
        .map(|s| s.as_str())
        .unwrap();
    let name = matches
        .get_one::<String>("NAME")
        .or_else(|| matches.get_one::<String>("NAME_POS"))
        .map(|s| s.as_str())
        .unwrap();

    if db.remove_alternative(name, target) {
        println!("update-alternatives: removed alternative {} for {}",
                 target, name);

        return true;
    }

    false
}

fn commit(db: &AlternativeDb) -> std::io::Result<()> {
    if let Err(e) = db.write_out("/etc/alternatives") {
        eprintln!("update-alternatives: could not commit changes to \
                  /etc/alternatives: {}", e);

        Err(e)
    } else if let Err(e) = db.write_links() {
        eprintln!("update-alternatives: could not write symlinks: {}", e);

        Err(e)
    } else {
        Ok(())
    }
}

fn run_gui(db: &mut AlternativeDb) -> bool {
    use std::process::Command;

    // Check for zenity
    let has_zenity = Command::new("sh")
        .arg("-c")
        .arg("command -v zenity >/dev/null 2>&1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !has_zenity {
        eprintln!("update-alternatives: --gui requested but 'zenity' was not found in PATH. Please install 'zenity' or run without --gui.");
        return false;
    }

    fn run_privileged(args: &[&str]) -> std::io::Result<std::process::ExitStatus> {
        let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("update-alternatives"));
        Command::new("pkexec").arg(&exe).args(args).status()
            .or_else(|_| Command::new("sudo").arg(&exe).args(args).status())
    }

    loop {
        let mut rows: Vec<(String, String)> = Vec::new();
        for (name, list) in db.iter() {
            let current = list
                .current_target()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| String::from("<none>"));
            rows.push((name.clone(), current));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));

        let menu_out = match Command::new("zenity").args([
            "--list", "--title", "update-alternatives",
            "--text", "Choose an action",
            "--width", "500", "--height", "300",
            "--column", "Action",
            "Add", "Remove", "Adjust priority", "Sync", "Close",
        ]).output() {
            Ok(o) => o,
            Err(e) => { eprintln!("update-alternatives: failed to launch zenity: {}", e); return false; }
        };
        if !menu_out.status.success() {
            return false; 
        }
        let choice = String::from_utf8_lossy(&menu_out.stdout).trim().to_string();
        match choice.as_str() {
            "Close" => return false,
            "Sync" => {
                match run_privileged(&["sync"]) {
                    Ok(s) if s.success() => { let _ = Command::new("zenity").args(["--info","--text","Symlinks were rewritten.","--title","update-alternatives"]).status(); }
                    Ok(s) => { let _ = Command::new("zenity").args(["--error","--text", &format!("Sync failed (exit {:?}).", s.code()), "--title","update-alternatives"]).status(); }
                    Err(e) => { let _ = Command::new("zenity").args(["--error","--text", &format!("Sync failed: {}", e), "--title","update-alternatives"]).status(); }
                }
            }
            "Add" => {
                let form = match Command::new("zenity").args([
                    "--forms", "--title", "Add alternative",
                    "--text", "Enter name, target and priority",
                    "--add-entry", "Name",
                    "--add-entry", "Target path",
                    "--add-entry", "Priority (integer)",
                    "--width", "500",
                ]).output() { Ok(o) => o, Err(e) => { eprintln!("zenity error: {}", e); return false; } };
                if !form.status.success() { continue; }
                let resp = String::from_utf8_lossy(&form.stdout).trim().to_string();
                let mut parts = resp.split('|');
                let name = parts.next().unwrap_or("").trim();
                let target = parts.next().unwrap_or("").trim();
                let weight = parts.next().unwrap_or("").trim();
                if name.is_empty() || target.is_empty() || weight.is_empty() { let _=Command::new("zenity").args(["--error","--text","All fields are required.","--title","update-alternatives"]).status(); continue; }
                if weight.parse::<i32>().is_err() { let _=Command::new("zenity").args(["--error","--text","Priority must be an integer.","--title","update-alternatives"]).status(); continue; }
                match run_privileged(&["add","-n", name, "-t", target, "-w", weight]) {
                    Ok(s) if s.success() => { let _=Command::new("zenity").args(["--info","--text","Alternative added/updated.","--title","update-alternatives"]).status(); }
                    Ok(s) => { let _=Command::new("zenity").args(["--error","--text", &format!("Add failed (exit {:?}).", s.code()), "--title","update-alternatives"]).status(); }
                    Err(e) => { let _=Command::new("zenity").args(["--error","--text", &format!("Add failed: {}", e), "--title","update-alternatives"]).status(); }
                }
            }
            "Remove" | "Adjust priority" => {
                if rows.is_empty() { let _=Command::new("zenity").args(["--warning","--text","No alternatives available.","--title","update-alternatives"]).status(); continue; }
                let mut name_list_args = vec!["--list","--title","Select name","--column","Name"]; 
                for (n, _) in &rows { name_list_args.push(n); }
                let name_out = match Command::new("zenity").args(&name_list_args).output() { Ok(o)=>o, Err(e)=>{ eprintln!("zenity error: {}", e); return false; } };
                if !name_out.status.success() { continue; }
                let selected_name = String::from_utf8_lossy(&name_out.stdout).trim().to_string();
                if selected_name.is_empty() { continue; }
                let mut alt_rows: Vec<(String, i32)> = Vec::new();
                if let Some(list) = db.alternatives(&selected_name) { for a in list.links() { alt_rows.push((a.target().display().to_string(), a.priority())); } }
                if alt_rows.is_empty() { let _=Command::new("zenity").args(["--warning","--text","No targets for this name.","--title","update-alternatives"]).status(); continue; }
                let mut alt_args: Vec<String> = vec!["--list".into(),"--title".into(),format!("{}: select target", selected_name),"--width".into(),"700".into(),"--column".into(),"Target".into(),"--column".into(),"Priority".into()];
                for (t, w) in &alt_rows { alt_args.push(t.clone()); alt_args.push(w.to_string()); }
                let alt_out = match Command::new("zenity").args(&alt_args).output() { Ok(o)=>o, Err(e)=>{ eprintln!("zenity error: {}", e); return false; } };
                if !alt_out.status.success() { continue; }
                let selected_target = String::from_utf8_lossy(&alt_out.stdout).trim().to_string();
                if selected_target.is_empty() { continue; }
                if choice == "Remove" {
                    match run_privileged(&["remove","-n", &selected_name, "-t", &selected_target]) {
                        Ok(s) if s.success() => { let _=Command::new("zenity").args(["--info","--text","Alternative removed.","--title","update-alternatives"]).status(); }
                        Ok(s) => { let _=Command::new("zenity").args(["--error","--text", &format!("Remove failed (exit {:?}).", s.code()), "--title","update-alternatives"]).status(); }
                        Err(e) => { let _=Command::new("zenity").args(["--error","--text", &format!("Remove failed: {}", e), "--title","update-alternatives"]).status(); }
                    }
                } else {
                    let pr_out = match Command::new("zenity").args(["--entry","--title","Set priority","--text","Enter new priority (integer)"]).output() { Ok(o)=>o, Err(e)=>{ eprintln!("zenity error: {}", e); return false; } };
                    if !pr_out.status.success() { continue; }
                    let new_w = String::from_utf8_lossy(&pr_out.stdout).trim().to_string();
                    if new_w.parse::<i32>().is_err() { let _=Command::new("zenity").args(["--error","--text","Priority must be an integer.","--title","update-alternatives"]).status(); continue; }
                    match run_privileged(&["add","-n", &selected_name, "-t", &selected_target, "-w", &new_w]) {
                        Ok(s) if s.success() => { let _=Command::new("zenity").args(["--info","--text","Priority updated.","--title","update-alternatives"]).status(); }
                        Ok(s) => { let _=Command::new("zenity").args(["--error","--text", &format!("Update failed (exit {:?}).", s.code()), "--title","update-alternatives"]).status(); }
                        Err(e) => { let _=Command::new("zenity").args(["--error","--text", &format!("Update failed: {}", e), "--title","update-alternatives"]).status(); }
                    }
                }
            }
            _ => { }
        }

        if let Ok(new_db) = read_db("/etc/alternatives") { *db = new_db; }
    }
}

fn sync(db: &AlternativeDb) -> bool {
    if let Err(e) = db.write_links() {
        eprintln!("update-alternatives: could not write symlinks: {}", e);
        std::process::exit(1);
    }

    false
}


fn app() -> clap::Command {
    use clap::{Arg, Command};
    Command::new("update-alternatives")
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(ABOUT)
        .arg(
            Arg::new("gui")
                .help("Launch a simple graphical interface for listing and syncing alternatives")
                .long("gui")
                .action(clap::ArgAction::SetTrue)
        )
        .subcommand(
            Command::new("list")
                .about(LIST_ABOUT)
                .arg(
                    Arg::new("NAME")
                        .help("The name of the alternatives to query")
                        .value_name("NAME")
                        .short('n')
                        .long("name")
                        .num_args(1)
                        .required_unless_present("NAME_POS")
                        .conflicts_with("NAME_POS"),
                )
                .arg(
                    Arg::new("NAME_POS")
                        .help("The name of the alternatives to query")
                        .value_name("NAME")
                        .index(1)
                        .required_unless_present("NAME")
                        .conflicts_with("NAME"),
                ),
        )
        .subcommand(
            Command::new("add")
                .about(ADD_ABOUT)
                .arg(
                    Arg::new("TARGET")
                        .help("The target of the alternative to add")
                        .value_name("TARGET")
                        .short('t')
                        .long("target")
                        .num_args(1)
                        .required_unless_present("TARGET_POS")
                        .conflicts_with("TARGET_POS"),
                )
                .arg(
                    Arg::new("NAME")
                        .help("The name of the alternative to add")
                        .value_name("NAME")
                        .short('n')
                        .long("name")
                        .num_args(1)
                        .required_unless_present("NAME_POS")
                        .conflicts_with("NAME_POS"),
                )
                .arg(
                    Arg::new("WEIGHT")
                        .help("The priority of the alternative to add")
                        .value_name("WEIGHT")
                        .short('w')
                        .long("weight")
                        .num_args(1)
                        .required_unless_present("WEIGHT_POS")
                        .conflicts_with("WEIGHT_POS"),
                )
                .arg(
                    Arg::new("NAME")
                        .help("The name of the alternative to add")
                        .value_name("NAME")
                        .index(1)
                        .required_unless_present("NAME")
                        .conflicts_with("NAME"),
                )
                .arg(
                    Arg::new("TARGET")
                        .help("The target of the alternative to add")
                        .value_name("TARGET")
                        .index(2)
                        .required_unless_present("TARGET")
                        .conflicts_with("TARGET"),
                )
                .arg(
                    Arg::new("WEIGHT")
                        .help("The priority of the alternative to add")
                        .value_name("WEIGHT")
                        .index(3)
                        .required_unless_present("WEIGHT")
                        .conflicts_with("WEIGHT"),
                ),
        )
        .subcommand(
            Command::new("remove")
                .about(REMOVE_ABOUT)
                .arg(
                    Arg::new("TARGET")
                        .help("The target of the alternative to remove")
                        .value_name("TARGET")
                        .short('t')
                        .long("target")
                        .num_args(1)
                        .required_unless_present("TARGET_POS")
                        .conflicts_with("TARGET_POS"),
                )
                .arg(
                    Arg::new("NAME")
                        .help("The name of the alternative to remove")
                        .value_name("NAME")
                        .short('n')
                        .long("name")
                        .num_args(1)
                        .required_unless_present("NAME_POS")
                        .conflicts_with("NAME_POS"),
                )
                .arg(
                    Arg::new("NAME")
                        .help("The name of the alternative to remove")
                        .value_name("NAME")
                        .index(1)
                        .required_unless_present("NAME")
                        .conflicts_with("NAME"),
                )
                .arg(
                    Arg::new("TARGET")
                        .help("The target of the alternative to remove")
                        .value_name("TARGET")
                        .index(2)
                        .required_unless_present("TARGET")
                        .conflicts_with("TARGET"),
                ),
        )
        .subcommand(Command::new("sync").about(SYNC_ABOUT))
        .subcommand_required(false)
        .arg_required_else_help(true)
        .propagate_version(true)
}

static ABOUT: &'static str =
    "Manages symlinks to be placed in /usr/local/bin. Data is stored in \
    /etc/alternatives for persistence between invocations. Provides similar \
    functionality to Debian's update-alternatives, but with a slightly \
    different interface. Alternatives are selected by comparing their assigned \
    priority values, with the highest priority being linked to. \
    Example usage to use 'vim' to open 'nvim'': \
    \nsudo update-alternatives add -n vim -t /usr/bin/nvim -w 100 ";

static LIST_ABOUT: &'static str =
    "Lists all alternatives for <NAME> and their assigned priority.";

static ADD_ABOUT: &'static str =
    "Adds or modifies an alternative for <NAME> that points to <TARGET> with \
    priority <WEIGHT>. If the database is modified, requires read/write access \
    to /etc/alternatives and /usr/local/bin.";

static REMOVE_ABOUT: &'static str =
    "If one exists, removes the alternative for <NAME> that points to \
    <TARGET>. If the database is modified, requires read/write access to \
    /etc/alternatives and /usr/local/bin.";

static SYNC_ABOUT: &'static str =
    "Rewrites all symlinks in /usr/local/bin based on the current state of \
    /etc/alternatives without modifying the database. Useful for package \
    manager hooks (e.g., pacman libalpm hooks) after installs, upgrades, or \
    removals.";
