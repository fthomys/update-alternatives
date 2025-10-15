// Copyright (c) 2018, Gregory Meyer
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

#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde_derive;

mod alternative;
mod alternative_db;
mod alternative_list;
mod filesystem;

use alternative::Alternative;
use alternative_db::AlternativeDb;

fn main() {
    let uid = nix::unistd::getuid();
    if uid.is_root() == false {
        eprintln!("update-alternatives: must be run as root");
        std::process::exit(1);
    }
    
    let matches = app().get_matches();

    let mut db = match read_db("/etc/alternatives") {
        Ok(d) => d,
        Err(_) => std::process::exit(1),
    };

    let mutated = match matches.subcommand() {
        Some(("list", sub_m)) => list(&db, sub_m),
        Some(("add", sub_m)) => add(&mut db, sub_m),
        Some(("remove", sub_m)) => remove(&mut db, sub_m),
        Some(("sync", _sub_m)) => sync(&db),
        _ => false,
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

fn sync(db: &AlternativeDb) -> bool {
    if let Err(e) = db.write_links() {
        eprintln!("update-alternatives: could not write symlinks: {}", e);
        std::process::exit(1);
    }

    false
}

use clap::Command;

fn app() -> clap::Command {
    use clap::{Arg, Command};
    Command::new("update-alternatives")
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(ABOUT)
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
                    Arg::new("NAME")
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
        .subcommand_required(true)
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
