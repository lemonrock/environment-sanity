// This file is part of environment-sanity. It is subject to the license terms in the COPYRIGHT file found in the top-level directory of this distribution and at https://raw.githubusercontent.com/lemonrock/environment-sanity/master/COPYRIGHT. No part of environment-sanity, including this file, may be copied, modified, propagated, or distributed except according to the terms contained in the COPYRIGHT file.
// Copyright © 2017 The developers of environment-sanity. See the COPYRIGHT file in the top-level directory of this distribution and at https://raw.githubusercontent.com/lemonrock/environment-sanity/master/COPYRIGHT.


#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![feature(command_envs)]


extern crate memchr;


use ::std::collections::HashMap;
use ::std::collections::HashSet;
use ::std::env::args_os;
use ::std::env::vars_os;
use ::std::ffi::OsString;
use ::std::fs::File;
use ::std::io::BufReader;
use ::std::io::prelude::*;
#[cfg(unix)] use ::std::os::unix::ffi::OsStringExt;
#[cfg(unix)] use ::std::os::unix::ffi::OsStrExt;
#[cfg(unix)] use ::std::os::unix::process::CommandExt;
use ::std::path::Path;
use ::std::process::Command;
use ::std::process::Stdio;


macro_rules! warn
{
	($message:tt, $($arg:tt)*) =>
	{
		{
			use ::std::io::Write;
			let result = writeln!(&mut ::std::io::stderr(), concat!("environment-sanity:WARN:", $message), $($arg)*);
			result.expect("Could not write line to stderr");
		}
	}
}

#[macro_export]
macro_rules! fatalExit
{
	($message:tt, $($arg:tt)*) =>
	{
		{
			use ::std::io::Write;
			let result = writeln!(&mut ::std::io::stderr(), concat!("environment-sanity:EXIT:", $message), $($arg)*);
			result.expect("Could not write line to stderr");
			::std::process::exit(1);
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvironmentVariable(OsString);

const AsciiNul: u8 = 0x00;

fn osStringFromRawBytesWithoutADelimiter(mut environmentVariableRawBytes: Vec<u8>) -> OsString
{
	environmentVariableRawBytes.push(AsciiNul);
	OsString::from_vec(environmentVariableRawBytes)
}

impl EnvironmentVariable
{
	pub fn fromRawBytesWithoutADelimiter(environmentVariableRawBytes: Vec<u8>) -> Self
	{
		EnvironmentVariable(osStringFromRawBytesWithoutADelimiter(environmentVariableRawBytes))
	}
	
	pub fn to_os_string(self) -> OsString
	{
		self.0
	}
}

impl<'a> From<&'a str> for EnvironmentVariable
{
	fn from(string: &'a str) -> Self
	{
		EnvironmentVariable(OsString::from(string))
	}
}

#[derive(Debug, Clone)]
pub struct BlackList(HashSet<EnvironmentVariable>);

impl BlackList
{
	pub fn new(defaultBlackList: Vec<EnvironmentVariable>) -> Self
	{
		let mut blackList = BlackList(HashSet::with_capacity(defaultBlackList.len() + 8));
		for environmentVariableName in defaultBlackList
		{
			blackList.0.insert(environmentVariableName);
		}
		blackList
	}
	
	pub fn addToFromFile(&mut self, filePath: &Path)
	{
		addEnvironmentVariablesFromLinesListedInFile("Black", filePath, |environmentVariableName, _, _|
		{
			self.0.insert(environmentVariableName);
		});
	}
	
	pub fn isBlackListed(&self, environmentVariableName: &EnvironmentVariable) -> bool
	{
		self.0.contains(environmentVariableName)
	}
	
	pub fn isNotBlackListed(&self, environmentVariableName: &EnvironmentVariable) -> bool
	{
		!self.isBlackListed(environmentVariableName)
	}
}

#[derive(Debug, Clone)]
pub struct WhiteList<'a>(HashSet<EnvironmentVariable>, &'a BlackList);

impl<'a> WhiteList<'a>
{
	pub fn new(blackList: &'a BlackList, defaultWhiteList: Vec<EnvironmentVariable>) -> Self
	{
		let mut whiteList = WhiteList(HashSet::with_capacity(defaultWhiteList.len() + 64), blackList);
		for environmentVariableName in defaultWhiteList
		{
			if whiteList.1.isBlackListed(&environmentVariableName)
			{
				fatalExit!("Environment variable '{:?}' occurs in the defaults for the black list AND the white list", environmentVariableName);
			}
			whiteList.0.insert(environmentVariableName);
		}
		whiteList
	}
	
	pub fn addToFromFile(&mut self, filePath: &Path)
	{
		addEnvironmentVariablesFromLinesListedInFile("White", filePath, |environmentVariableName, filePath, line|
		{
			if self.1.isBlackListed(&environmentVariableName)
			{
				warn!("Black list contains environment variable '{:?}' white listed in file '{:?}' at line '{}' (all offsets are zero-based)", environmentVariableName, filePath, line);
			}
			else
			{
				self.0.insert(environmentVariableName);
			}
		});
	}
	
	pub fn isWhiteListed(&self, environmentVariableName: &EnvironmentVariable) -> bool
	{
		self.0.contains(environmentVariableName)
	}
	
	pub fn filterEnvironment(&self) -> HashMap<OsString, OsString>
	{
		let blackList = self.1;
		vars_os()
		.filter(|&(ref environmentVariableName, _)| blackList.isNotBlackListed(&EnvironmentVariable(environmentVariableName.to_os_string())))
		.filter(|&(ref environmentVariableName, _)| self.isWhiteListed(&EnvironmentVariable(environmentVariableName.to_os_string())))
		.collect()
	}
}

#[derive(Debug, Clone)]
pub struct SettingsList(HashMap<EnvironmentVariable, OsString>);

impl SettingsList
{
	pub fn new(defaultSettingsList: HashMap<EnvironmentVariable, OsString>) -> Self
	{
		SettingsList(defaultSettingsList)
	}
	
	pub fn addSettingsToEnvironment(self, mut environment: HashMap<OsString, OsString>) -> HashMap<OsString, OsString>
	{
		for (environmentVariableName, value) in self.0
		{
			environment.insert(environmentVariableName.to_os_string(), value);
		}
		environment
	}
	
	pub fn addToFromFile(&mut self, filePath: &Path)
	{
		addFromLinesListedInFile("Settings", filePath, |environmentVariableRawBytesExcludingDelimiter, fileKind, filePath, line|
		{
			const Tab: u8 = b'\t';
			match memchr::memchr(Tab, environmentVariableRawBytesExcludingDelimiter.as_slice())
			{
				None => fatalExit!("There is no tab delimiter in {} list file '{:?}' at line '{}' (all offsets are zero-based)", fileKind, filePath, line),
				Some(index) =>
				{
					let name = EnvironmentVariable::fromRawBytesWithoutADelimiter(Vec::from(&environmentVariableRawBytesExcludingDelimiter[0..index]));
					let value = osStringFromRawBytesWithoutADelimiter(Vec::from(&environmentVariableRawBytesExcludingDelimiter[index + 1..]));
					
					self.0.insert(name, value);
				}
			}
		})
	}
}

/// This logic only works if there are not any LineFeed characters EMBEDDED within a line
/// This logic does not play nice on Windows with NotePad (which insists on using CRLF to end lines), but it does allow commonality of definitions
fn addFromLinesListedInFile<A: FnMut(Vec<u8>, &'static str, &Path, u64)>(fileKind: &'static str, filePath: &Path, mut add: A)
{
	let file = match File::open(filePath)
	{
		Ok(file) => file,
		Err(_) => fatalExit!("Could not open {} list file '{:?}' for reading", fileKind.to_lowercase(), filePath),
	};
	
	let bufferedReader = BufReader::with_capacity(4096, file);
	
	const LineFeed: u8 = 0x0A;
	let mut line = 0;
	for environmentVariableRawBytesExcludingDelimiter in bufferedReader.split(LineFeed)
	{
		match environmentVariableRawBytesExcludingDelimiter
		{
			Err(_) => fatalExit!("Could not read line '{}' in {} list file '{:?}' (all offsets are zero-based)", line, fileKind.to_lowercase(), filePath),
			Ok(environmentVariableRawBytes) =>
			{
				if let Some(column) = memchr::memchr(AsciiNul, environmentVariableRawBytes.as_slice())
				{
					fatalExit!("{} list file '{:?}' at line '{}' contains an ASCII NUL at column '{}' (all offsets are zero-based)", fileKind, filePath, line, column)
				}
				
				add(environmentVariableRawBytes, fileKind, filePath, line)
			},
		}
		line += 1;
	}
}

fn addEnvironmentVariablesFromLinesListedInFile<A: FnMut(EnvironmentVariable, &Path, u64)>(fileKind: &'static str, filePath: &Path, mut add: A)
{
	addFromLinesListedInFile(fileKind, filePath, |environmentVariableRawBytesExcludingDelimiter, _, filePath, line|
	{
		let environmentVariableName = EnvironmentVariable::fromRawBytesWithoutADelimiter(environmentVariableRawBytesExcludingDelimiter);
		add(environmentVariableName, filePath, line);
	});
}

pub fn parseCommandLineArguments() -> (OsString, Vec<OsString>)
{
	// This logic is designed to work with sha-bang paths, eg
	// /usr/bin/environment-sanity program-to-invoke <any> <other> <arguments>
	// sha-bang paths as used as a command interpreter may not support <any> <other> <arguments>
	
	// Skip the first argument, which is 'us'
	let mut inputArguments = args_os().skip(1);
	
	// Take the second argument, which is the program to invoke
	let programName = match inputArguments.next()
	{
		None => fatalExit!("Please provide at least one argument, which is the program to {}", "invoke"),
		Some(programName) =>
		{
			if programName.is_empty()
			{
				fatalExit!("{}", "First argument can not be empty");
			}
			
			const Slash: u8 = b'/';
			if memchr::memchr(Slash, programName.as_os_str().as_bytes()).is_some()
			{
				fatalExit!("First argument is the program name to invoke. It must be a file, not a path like '{:?}'", programName);
			}
			
			programName
		}
	};
	
	let outputArguments = inputArguments.collect();
	
	(programName, outputArguments)
}

pub fn execute(programName: OsString, arguments: Vec<OsString>, filteredEnvironment: HashMap<OsString, OsString>) -> !
{
	let error = Command::new(&programName)
	.stdin(Stdio::inherit())
	.stdout(Stdio::inherit())
	.stderr(Stdio::inherit())
	.args(&arguments)
	.env_clear().envs(&filteredEnvironment)
	.exec();
	
	fatalExit!("Could not execute '{:?}' because '{:?}'", programName, error);
}
