# CLI docs

You can access this information from the CLI itself using the help subcommand or help flag (this document is, in fact, ripped directly from there).

<!-- vscode-markdown-toc -->
* 1. [Usage](#Usage)
* 2. [Subcommands](#Subcommands)
* 3. [`config` subcommand](#configsubcommand)
  * 3.1. [Usage](#Usage-1)
  * 3.2. [Options](#Options)
  * 3.3. [Examples](#Examples)
* 4. [`devices` subcommand](#devicessubcommand)
  * 4.1. [Usage](#Usage-1)
  * 4.2. [Options](#Options-1)
  * 4.3. [Examples](#Examples-1)
* 5. [`help` subcommand](#helpsubcommand)
  * 5.1. [Usage](#Usage-1)
  * 5.2. [Examples](#Examples-1)
* 6. [`run` subcommand](#runsubcommand)
  * 6.1. [Usage](#Usage-1)
  * 6.2. [Options](#Options-1)
  * 6.3. [Examples](#Examples-1)

<!-- vscode-markdown-toc-config
	numbering=true
	autoSave=true
	/vscode-markdown-toc-config -->
<!-- /vscode-markdown-toc -->

## 1. <a name='Usage'></a>Usage

```bash
 midiboard <SUBCOMMAND>
```

This utility helps with the execution of frequent or specific tasks to be done using a MIDI controller to execute user-provided commands.

It can be used to control audio, system resoruces, or anything that runs off a shell command.

## 2. <a name='Subcommands'></a>Subcommands

| subcommand | Description                                                                           |
|------------|---------------------------------------------------------------------------------------|
| `config`   | Manages the configuration file.                                                       |
| `devices`  | Detects and listens to currently active MIDI devices.                                 |
| `help`     | Print this message or the help of the given subcommand(s)                             |
| `run`      | Runs the service, listening to incoming events and executing the given configuration. |

## 3. <a name='configsubcommand'></a>`config` subcommand

### 3.1. <a name='Usage-1'></a>Usage

```bash
 midiboard config [OPTIONS]
```

This command allows you to generate a skeleton for the config file, or devices validity of an existing one.

By default the configuration file will be generated and read from `$HOME`, but you can select an alternative path if desired.

### 3.2. <a name='Options'></a>Options

| Short | Long         | Takes value | Description                                |
|-------|--------------|-------------|--------------------------------------------|
| `-g`  | `--generate` | -           | Generates a skeleton config file.          |
| `-h`  | `--help`     | -           | Print help information.                    |
| `-p`  | `--path`     | String      | Selects a custom path for the config file. |
| `-v`  | `--validate` | -           | Validates the config file.                 |

### 3.3. <a name='Examples'></a>Examples

```bash
 midiboard config --generate
```

```bash
 midiboard config --generate --path ./midiboard.json
```

```bash
 midiboard config --validate
```

```bash
 midiboard config --validate --path ./midiboard.json
```

## 4. <a name='devicessubcommand'></a>`devices` subcommand

### 4.1. <a name='Usage-1'></a>Usage

```bash
 midiboard devices [OPTIONS]
```

This command lets you know what devices you have active, their names, and check if they're working correctly.

It can provide a list of devices, and with a device selected it can output any MIDI event registered. This is useful to know what channels, keys and type of values your device outputs, making easy to fill the config file.

### 4.2. <a name='Options-1'></a>Options

| Short | Long      | Takes value | Description                                                        |
|-------|-----------|-------------|--------------------------------------------------------------------|
| `-l`  | `--list`  | -           | Lists active MIDI devices and outputs them to stdout.              |
| `-h`  | `--help`  | -           | Print help information.                                            |
| `-i`  | `--input` | String      | Listens to the given MIDI device and outputs all events to stdout. |

### 4.3. <a name='Examples-1'></a>Examples

```bash
 midiboard devices --list
```

```bash
 midiboard devices --input "Arturia Beatstep"
```

## 5. <a name='helpsubcommand'></a>`help` subcommand

### 5.1. <a name='Usage-1'></a>Usage

```bash
 midiboard help <SUBCOMMAND>
```

### 5.2. <a name='Examples-1'></a>Examples

```bash
 midiboard help config
```

```bash
 midiboard help devices
```

## 6. <a name='runsubcommand'></a>`run` subcommand

### 6.1. <a name='Usage-1'></a>Usage

```bash
 midiboard run [OPTIONS]
```

Executes given commands on defined MIDI events according to the config file.

By default the configuration file will be generated and read from $HOME, but you can select an alternative path if desired.

### 6.2. <a name='Options-1'></a>Options

| Short | Long     | Takes value | Description                                |
|-------|----------|-------------|--------------------------------------------|
| `-p`  | `--path` | String      | Selects a custom path for the config file. |
| `-h`  | `--help` | -           | Print help information.                    |

### 6.3. <a name='Examples-1'></a>Examples

```bash
 midiboard run
```

```bash
 midiboard run --path ./midiboard.json
```
