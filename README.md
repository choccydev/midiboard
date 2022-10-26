
# MidiBoard

CLI tool to trigger arbitrary commands using any MIDI 1.0 compatible  device.

## What is this?

This tool lets the user associate any executable commands to actions in one or more MIDI devices, like changing volume, altering backlight level, changing a song, starting/stopping a service, or launching any script/executable, on the press of a button or turn of a knob.

## Getting Started

To use this software you need to install it, define a configuration, and then either run at-will or daemonize the process. The most cumbersome part, of course is writing the configuration file.

For details check [the docs](https://github.com/aordano/midiboard/tree/master/docs).

## Install

### Cargo

   ```bash
   cargo install midiboard
   ```

### Manual

1. Clone

   ```bash
   git clone https://github.com/aordano/midiboard.git
   ```

2. Build

   ```bash
   cargo build --release
   ```

3. Copy binary

   ```bash
   sudo cp ./target/release/midiboard /usr/bin/midiboard
   ```

## Configuration

For details on the configuration file check the [config docs page](https://github.com/aordano/midiboard/blob/master/docs/config.md).

For help using the CLI, there is integrated help via the `--help` flag.

### Create a skeleton

 This will create it at `$HOME/midiboard.json`. Optionally add a `--path` flag to change the output location:

   ```bash
   midiboard config --generate
   ```

### Define your device

Get the name of your device and put it in the `device` field in the config:

   ```bash
   midiboard devices --list
   ```

### Define each command

Listen to the input to know what Is the numerical key value of your chosen knob/button/key:

   ```bash
   midiboard devices --input [DEVICE_NAME]
   ```

   With that value you can fill the corresponding entry on the config file.

## Running

### Manually

By default it will expect a config file at `$HOME/midiboard.json`.Optionally add a `--path` flag to change the output location:

   ```bash
   midiboard run
   ```

### Daemonized

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
