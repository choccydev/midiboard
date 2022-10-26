# The configuration file

<!-- vscode-markdown-toc -->
* 1. [Concept](#Concept)
* 2. [Configuration object](#Configurationobject)
  * 2.1. [`device`](#device)
  * 2.2. [`thresholds`](#thresholds)
* 3. [Controls](#Controls)
* 4. [Command](#Command)
  * 4.1. [Command definition object](#Commanddefinitionobject)
  * 4.2. [`Encoder` kind](#Encoderkind)
  * 4.3. [`Switch` kind](#Switchkind)
  * 4.4. [`Trigger` kind](#Triggerkind)
* 5. [The full tree](#Thefulltree)
  * 5.1. [Config entry tree](#Configentrytree)
  * 5.2. [Thresholds tree](#Thresholdstree)
  * 5.3. [Controls tree](#Controlstree)
* 6. [Even more detail](#Evenmoredetail)

<!-- vscode-markdown-toc-config
	numbering=true
	autoSave=true
	/vscode-markdown-toc-config -->
<!-- /vscode-markdown-toc -->

This is a `JSON` file, named `midiboard.json`, that by default is expected to be located at `$HOME/midiboard.json`.

This documentation will define the data model and how to understand and write this file. It is heavily recommended to use the integrated tool to generate a skeleton, because it has a JSON Schema definition with descriptions for every key and some basic type/bounds checking.

## 1. <a name='Concept'></a>Concept

The idea of having a complex configuration for this program is to let the user define actions as code, having free reign into what one can do on actuation of a key, button or knob in the selected controller or controllers.

The program is designed to allow multiple devices with their own rules and definitions, keeping a runtime for each device on a separated thread. This means that if you want to have different configurations for completely different devices, all of it can live on a single file, keeping simple the daemonization process.

Tho achieve this goals, the `JSON` is structured to contain the schema location and a single key that contains an array with all the whole config entries:

| Property | Type          | Description                                         |
|----------|---------------|-----------------------------------------------------|
| `$Schema`  | String (enum) | Schema URI of the `JSON` file.                          |
| `config`   | Array (Object)        | List of all the config definitions, one per device. |

## 2. <a name='Configurationobject'></a>Configuration object

Each config entry in the `config` array is an object, representing the whole config per each device. There can only be one device per object in the array.

The individual configuration is separated in three parts; `device`, `thresholds`,  and `controls`:

| Property  | Type   | Description                                                       |
|-----------|--------|-------------------------------------------------------------------|
| `device`    | String | Name of the device.                                               |
| `thresholds` | Object | Set of time thresholds for activating different kind of controls. |
| `controls`  | Object | Set of controls and its actions on activation.                    |

### 2.1. <a name='device'></a>`device`

Simply the name of the device as reported by ALSA. This value must be exact. To get the name you can either use the included tool `midiboard devices --list`, or use `aseqdump -l`.

### 2.2. <a name='thresholds'></a>`thresholds`

List of thresholds. for every type of event. For more information on available events, thresholds, and how they differentiate, check the [events docs](https://github.com/aordano/midiboard/docs/events.md).

| Property | Type   | Description                              |
|----------|--------|------------------------------------------|
| `encoder`  | Object | Threshold data for any `Encoder` event.  |
| `trigger` | Object | Threshold data for any `Trigger` event.  |
| `switch`   | Object | Threshold data for any  `Switch`  event. |

Each threshold data object contains two keys, `detection`, and `activation`:

| Property   | Type   | Description                                                                                              |
|------------|--------|----------------------------------------------------------------------------------------------------------|
| `detection`  | Number | Minimum time, annotated in `ms`, for considering a detection event as successful.                        |
| `activation` | Number | Minimum time, annotated in `ms`, for considering an activation for any control that has this event type. |

## 3. <a name='Controls'></a>Controls

The `controls` key is where the meat of the config file is located.

Each child property (_you can name them as you want, as long as you only use a single word, in lowercase and/or digits, starting with a letter (not a digit), low dashes are ok too._) will be a chosen name for a given control.

| Property         | Type   | Description                                 |
|------------------|--------|---------------------------------------------|
| control name | Object | Object defining the given control behavior. |

Each control has three properties; `key`, `kind`, and `command`:

| Property  | Type          | Description                                                                                       |
|-----------|---------------|---------------------------------------------------------------------------------------------------|
| `key`     | Number        | Numeric value representing the key associated with this control, on the corresponding device.     |
| `command` | Object        | Object defining the command(s) to execute on a successful activation of the control.              |

You can get the value of the `key` (the activated controller on the midi device) using the included tool `midiboard devices --input <DEVICE_NAME>` or with `aseqdump -p <PORT_NUMBER>`.

## 4. <a name='Command'></a>Command

This entry has three sets of possible children. What children does it have depends on the kind of event associated to the command. It always has a `kind` property that defines what type of event is associated with, and the correct keys for the command data.

| Property     | Type          | Description                                                                                                   |
|--------------|---------------|---------------------------------------------------------------------------------------------------------------|
| `kind`       | String (enum) | Event type to understand the control behavior as. Options are  ` Encoder ` ,  ` Switch ` , and  ` Trigger ` . |
|  valid keys | Object        | Valid key depends on selected `kind` of event.                                                                |

For more information on the event types check the [events docs](https://github.com/aordano/midiboard/blob/master/docs/events.md#Eventtypes).

### 4.1. <a name='Commanddefinitionobject'></a>Command definition object

Every one of the keys described as "Command definition object" contain this common object describing what command to execute:

| Property | Type           | Description                                                            |
|----------|----------------|------------------------------------------------------------------------|
| `cmd`    | String         | Main command to execute. Must be in `$PATH` or a script file location. |
| `args`   | Array (String) | List of arguments to add to the given command.                         |

### 4.2. <a name='Encoderkind'></a>`Encoder` kind

Encoders execute different commands on rising values (turning a knob or sliding a fader incrementing the detected value) or falling values (turning a knob or sliding a fader decreasing the detected value).

| Property | Type          | Description                                                                 |
|----------|---------------|-----------------------------------------------------------------------------|
| `kind`   | String (enum) | Event type to understand the control behavior as. Selected as ` Encoder ` . |
| `increase` | Object        | Command definition object for executing on detection of a rising value.     |
| `decrease` | Object        | Command definition object for executing on detection of a falling value.    |

### 4.3. <a name='Switchkind'></a>`Switch` kind

Switchs hold a binary state and alternate between both states, executing a different command on every state change.

| Property        | Type          | Description                                                                     |
|-----------------|---------------|---------------------------------------------------------------------------------|
| `kind`          | String (enum) | Event type to understand the control behavior as. Selected as `Switch`.         |
| `on`            | Object        | Command definition object for executing on setting the state as `ON`.           |
| `off`           | Object        | Command definition object for executing on setting the state as `OFF`.          |
| `initial_state` | String (enum) | Initial state to consider the control as being in, at the start of the program. |

### 4.4. <a name='Triggerkind'></a>`Trigger` kind

Triggers just execute a single command on a successful activation.

| Property  | Type          | Description                                                                      |
|-----------|---------------|----------------------------------------------------------------------------------|
| `kind`    | String (enum) | Event type to understand the control behavior as. Selected as `Trigger`.         |
| `execute` | Object        | Command definition object for executing on successful activation of the control. |

## 5. <a name='Thefulltree'></a>The full tree

To make it clearer and to more easily understand the big picture, i added some diagrams that encapsulate the config hierarchy tree:

### 5.1. <a name='Configentrytree'></a>Config entry tree

```mermaid
graph TD
  root(("{ }"))
  schema("$Schema") --> A((String))
  a.config("config") --->|" entries "| config("{ ... } ...")
  device("device") --> B((String))
  thresholds(["thresholds"])
  controls(["controls"])



  root --> schema
  root --> a.config

  config --> device
  config --> thresholds
  config --> controls

```

### 5.2. <a name='Thresholdstree'></a>Thresholds tree

```mermaid
graph TD
  thresholds(["thresholds"])
  thresh.encoder("encoder  ")
  thresh.switch("switch  ")
  thresh.trigger("trigger  ")
  
  thresh.sub[["THRESHOLD"]]
  thresh.sub.det("detection") --> A((Number))
  thresh.sub.act("activation") --> B((Number))
  thresh.sub --> thresh.sub.det
  thresh.sub --> thresh.sub.act

  thresholds --> thresh.encoder
  thresholds --> thresh.switch
  thresholds --> thresh.trigger

  thresh.encoder --> thresh.encoder.1[["THRESHOLD"]]
  thresh.switch --> thresh.switch.1[["THRESHOLD"]]
  thresh.trigger --> thresh.trigger.1[["THRESHOLD"]]

```

### 5.3. <a name='Controlstree'></a>Controls tree

```mermaid
graph TD
  controls(["controls"])
  controls.name("{ ... }")
  controls.key("key") --> A((String))
  command("command  ")

  controls --->|properties| controls.name
  controls.name --> command
  controls.name --> controls.key

  command --> choose{" "}
  choose1("{ ... }")
  choose2("{ ... }")
  choose3("{ ... }")
  choose ----->|Encoder| choose1
  choose -------->|Switch| choose2
  choose -->|Trigger| choose3

  choose1 --> kind1("kind") --> B1(("String: 'Encoder'"))
  choose1 --> enc1("increase") --> B2[["COMMAND"]]
  choose1 --> enc2("decrease") --> B3[["COMMAND"]]

  choose2 --> kind2("kind") --> C1(("String: 'Switch'"))
  choose2 --> sw1("on") --> C2[["COMMAND"]]
  choose2 --> sw2("off") --> C3[["COMMAND"]]
  choose2 --> sw3("initial_state") --> C4((String))

  choose3 --> kind3("kind") --> D1(("String: 'Trigger'"))
  choose3 --> tr1("execute") --> D2[["COMMAND"]]


  COMMAND[[COMMAND]] --> cmd1("cmd") --> cmd11((String))
  COMMAND --> cmd2("args") --> cmd21(("Array(String)"))

```

## 6. <a name='Evenmoredetail'></a>Even more detail

For more details you should directly check the [schema and its annotations](https://github.com/aordano/midiboard/blob/master/schema/midiboard.schema.json). It is recommended to use a `JSON` visualization tool to make more sense of it.
