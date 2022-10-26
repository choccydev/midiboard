# MidiBoard

CLI tool to trigger arbitrary commands using any MIDI 1.0 compatible  device.

<!-- vscode-markdown-toc -->
* 1. [What is this?](#Whatisthis)
* 2. [Getting Started](#GettingStarted)
* 3. [Install](#Install)
  * 3.1. [Cargo](#Cargo)
  * 3.2. [Manual](#Manual)
* 4. [Configuration](#Configuration)
  * 4.1. [Create a skeleton](#Createaskeleton)
  * 4.2. [Define your device](#Defineyourdevice)
  * 4.3. [Define each command](#Defineeachcommand)
* 5. [Running](#Running)
  * 5.1. [Manually](#Manually)
  * 5.2. [Daemonized](#Daemonized)

<!-- vscode-markdown-toc-config
	numbering=true
	autoSave=true
	/vscode-markdown-toc-config -->
<!-- /vscode-markdown-toc -->

## 1. <a name='Whatisthis'></a>What is this?

This tool lets the user associate any executable commands to actions in one or more MIDI devices, like changing volume, altering backlight level, changing a song, starting/stopping a service, or launching any script/executable, on the press of a button or turn of a knob.

## 2. <a name='GettingStarted'></a>Getting Started

To use this software you need to install it, define a configuration, and then either run at-will or daemonize the process. The most cumbersome part, of course is writing the configuration file.

For details check [the docs](https://github.com/aordano/midiboard/tree/master/docs).

## 3. <a name='Install'></a>Install

### 3.1. <a name='Cargo'></a>Cargo

   ```bash
   cargo install midiboard
   ```

### 3.2. <a name='Manual'></a>Manual

1. Clone

   ```bash
   git clone https://github.com/aordano/midiboard.git
   ```

2. Build

   ```bash
   cd midiboard
   cargo build --release
   ```

3. Copy binary

   ```bash
   sudo cp ./target/release/midiboard /usr/bin/midiboard
   ```

## 4. <a name='Configuration'></a>Configuration

For details on the configuration file check the [config docs page](https://github.com/aordano/midiboard/blob/master/docs/config.md).

For help using the CLI, there is integrated help via the `--help` flag.

### 4.1. <a name='Createaskeleton'></a>Create a skeleton

 This will create it at `$HOME/midiboard.json`. Optionally add a `--path` flag to change the output location:

   ```bash
   midiboard config --generate
   ```

### 4.2. <a name='Defineyourdevice'></a>Define your device

Get the name of your device and put it in the `device` field in the config:

   ```bash
   midiboard devices --list
   ```

### 4.3. <a name='Defineeachcommand'></a>Define each command

Listen to the input to know what Is the numerical key value of your chosen knob/button/key:

   ```bash
   midiboard devices --input [DEVICE_NAME]
   ```

   With that value you can fill the corresponding entry on the config file.

## 5. <a name='Running'></a>Running

### 5.1. <a name='Manually'></a>Manually

By default it will expect a config file at `$HOME/midiboard.json`.Optionally add a `--path` flag to change the output location:

   ```bash
   midiboard run
   ```

### 5.2. <a name='Daemonized'></a>Daemonized

1. Get the service file

   ```bash
   wget -O midiboard.service https://raw.githubusercontent.com/aordano/midiboard/master/schema/midiboard.service
   ```

2. Move the service file

   ```bash
   sudo mv midiboard.service /etc/systemd/system/midiboard.service
   ```

   The file uses the default config location. Modify the service file if you have an alternate path for the config file (add the `--path` flag).

3. Enable the service

   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now midiboard
   ```
