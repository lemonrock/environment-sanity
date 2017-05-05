// This file is part of environment-sanity. It is subject to the license terms in the COPYRIGHT file found in the top-level directory of this distribution and at https://raw.githubusercontent.com/lemonrock/environment-sanity/master/COPYRIGHT. No part of environment-sanity, including this file, may be copied, modified, propagated, or distributed except according to the terms contained in the COPYRIGHT file.
// Copyright © 2017 The developers of environment-sanity. See the COPYRIGHT file in the top-level directory of this distribution and at https://raw.githubusercontent.com/lemonrock/environment-sanity/master/COPYRIGHT.


#![allow(non_snake_case)]


#[macro_use] extern crate environment_sanity;


use ::environment_sanity::*;
use ::std::collections::HashMap;
use ::std::env::home_dir;
use ::std::env::remove_var;
use ::std::env::var_os;
use ::std::ffi::OsStr;
use ::std::ffi::OsString;
use ::std::path::PathBuf;


pub fn main()
{
	homeFolderIgnoringValueOfHomeVariable();
	
	let (programName, outputArguments) = parseCommandLineArguments();
	
	let mut blackList = BlackList::new(defaultBlackList());
	if let Some(filePath) = settingsFor(&programName, "Black")
	{
		blackList.addToFromFile(&filePath)
	}
	
	let mut whiteList = WhiteList::new(&blackList, defaultWhiteList());
	if let Some(filePath) = settingsFor(&programName, "White")
	{
		whiteList.addToFromFile(&filePath)
	}
	
	let mut settingsList = SettingsList::new(defaultSettings(&programName));
	if let Some(filePath) = settingsFor(&programName, "Settings")
	{
		settingsList.addToFromFile(&filePath)
	}
	
	let filteredEnvironment = whiteList.filterEnvironment();
	let environment = settingsList.addSettingsToEnvironment(filteredEnvironment);
	execute(programName, outputArguments, environment);
}

fn homeFolderIgnoringValueOfHomeVariable() -> PathBuf
{
	remove_var("HOME");
	match home_dir()
	{
		None => fatalExit!("{}", "Can not determine home directory"),
		Some(folderPath) => folderPath,
	}
}

fn settingsFor(programName: &OsStr, fileKind: &'static str) -> Option<PathBuf>
{
	let mut settingsFolderPath = homeFolderIgnoringValueOfHomeVariable();
	settingsFolderPath = settingsFolderPath.join(".environment-sanity/settings");
	settingsFolderPath = settingsFolderPath.join(PathBuf::from(programName));
	settingsFolderPath = settingsFolderPath.join(PathBuf::from(fileKind.to_lowercase()));
	if settingsFolderPath.exists() && settingsFolderPath.is_file()
	{
		Some(settingsFolderPath)
	}
	else
	{
		None
	}
}

fn defaultBlackList() -> Vec<EnvironmentVariable>
{
	vec!
	[
		// Who in their right mind thought this was a good idea?
		"CDPATH".into(),
		
		"LD_LIBRARY_PATH".into(),
		"LD_PRELOAD".into(),
		
		"DYLD_LIBRARY_PATH".into(),
		"DYLD_FALLBACK_LIBRARY_PATH".into(),
		"DYLD_FRAMEWORK_PATH".into(),
		"DYLD_FALLBACK_FRAMEWORK_PATH".into(),
		
		// glibc noise
		"LD_BIND_NOW ".into(),
		"LD_TRACE_LOADED_OBJECTS".into(),
		"LD_AOUT_LIBRARY_PATH".into(),
		"LD_AOUT_PRELOAD".into(),
		"LD_AUDIT".into(),
		"LD_BIND_NOT".into(),
		"LD_DEBUG".into(),
		"LD_DEBUG_OUTPUT".into(),
		"LD_DYNAMIC_WEAK".into(),
		"LD_HWCAP_MASK".into(),
		"LD_KEEPDIR".into(),
		"LD_NOWARN".into(),
		"LD_ORIGIN_PATH".into(),
		"LD_POINTER_GUARD".into(),
		"LD_PROFILE".into(),
		"LD_PROFILE_OUTPUT".into(),
		"LD_SHOW_AUXV".into(),
		"LD_USE_LOAD_BIAS".into(),
		"LD_VERBOSE".into(),
		"LD_WARN".into(),
		"LDD_ARGV0 ".into(),
		
		// These variables should both be identical but are too brittle to use; just eliminate them and rely of a syscall (which is accurate)
		// LOGNAME is used by musl for the getlogin() function
		"LOGNAME".into(),
		"USER".into(),
		
		// Likewise, do not pass down any SUDO information
		"SUDO_USER".into(),
		"SUDO_UID".into(),
		"SUDO_COMMAND".into(),
		"SUDO_GID".into(),
		
		// Rust will try to use getpwuid_r() if HOME is not set
		"HOME".into(),
	]
}

fn defaultWhiteList() -> Vec<EnvironmentVariable>
{
	vec!
	[
		"PATH".into(),
		"TMPDIR".into(), // We should consider using a path under the user's home instead; Rust's std::env::temp_dir() defaults even to a non-extant '/tmp'!
		
		// PWD - without this, musl's implementation of get_current_dir_name() falls back to getcwd()
		
		//"SSH_CONNECTION".into(),
		//"SSH_AUTH_SOCK".into(),
		
		//"NLSPATH".into(),
		//"DATEMSK".into(),
		//"MSGVERB".into(),
		// MUSL_LOCPATH
		
		//"TERM".into(),
		//"COLUMNS".into(),
		//"LINES".into(),
		//"DISPLAY".into(),
		//"EDITOR".into(),
		//"VISUAL".into(),
	]
}

fn defaultSettings(programName: &OsStr) -> HashMap<EnvironmentVariable, OsString>
{
	let mut settings = HashMap::new();
	
	// There is no good reason for a time zone to be anything other than UTC by default
	settings.insert("TZ".into(), "Etc/UTC".into());
	
	// Forces HOME to match values in /etc/passwd
	settings.insert("HOME".into(), homeFolderIgnoringValueOfHomeVariable().into_os_string());
	
	// Forces TMPDIR to be local to the user; this is the most secure setting. The user is still free to use a symlink back to /tmp
	settings.insert("TMPDIR".into(),
	{
		let mut homeFolder = homeFolderIgnoringValueOfHomeVariable();
		homeFolder = homeFolder.join(PathBuf::from(".environment-sanity/tmp"));
		homeFolder = homeFolder.join(PathBuf::from(programName));
		homeFolder
	}.into_os_string());
	
	// Belt-and-braces approach to something that is deeply flawed
	settings.insert("HOMEBREW_NO_ANALYTICS".into(), "1".into());
	
	// Musl only supports C.UTF-8 or POSIX; Mac OS X only UTF-8
	// We set all these as belt-and-braces as a lot of code doesn't really grok the subtle POSIX rules for them and they 'break' stuff like `sort`
	// See here for a discussion of correct settings for Mac OS X and the BSDs: https://www.python.org/dev/peps/pep-0538/
	// glibc raw, as opposed to being used in Debian, etc, did not support C.UTF-8 correctly in 2014; this may still be the case.
	
	if cfg!(target_os = "linux")
	{
		settings.insert("LC_ALL".into(), "C.UTF-8".into());
		settings.insert("LC_COLLATE".into(), "C.UTF-8".into());
		settings.insert("LC_CTYPE".into(), "C.UTF-8".into());
		settings.insert("LC_MESSAGES".into(), "C.UTF-8".into());
		settings.insert("LC_MONETARY".into(), "C.UTF-8".into());
		settings.insert("LC_NUMERIC".into(), "C.UTF-8".into());
		settings.insert("LC_TIME".into(), "C.UTF-8".into());
		settings.insert("LANG".into(), "C.UTF-8".into());
		
		// BSDs introduced USER
		if let Some(logname) = var_os("LOGNAME")
		{
			settings.insert("USER".into(), logname);
		}
	}
	else
	{
		settings.insert("LC_ALL".into(), "UTF-8".into());
		settings.insert("LC_COLLATE".into(), "UTF-8".into());
		settings.insert("LC_CTYPE".into(), "UTF-8".into());
		settings.insert("LC_MESSAGES".into(), "UTF-8".into());
		settings.insert("LC_MONETARY".into(), "UTF-8".into());
		settings.insert("LC_NUMERIC".into(), "UTF-8".into());
		settings.insert("LC_TIME".into(), "UTF-8".into());
		settings.insert("LANG".into(), "UTF-8".into());
		
		// BSDs introduced USER
		if let Some(user) = var_os("USER")
		{
			settings.insert("LOGNAME".into(), user);
		}
	}
	
	
	settings
}
