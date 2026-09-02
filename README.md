This is koditerm, a terminal user interface for playing music from a Kodi server. It can control the remote 
Kodi system or it can stream the music locally. The project is written in Rust.

If koditerm can't playback locally, consider using the --device command line argument to specify an output 
device.

The program creates a default config file and populates it with some information, but there is config.toml.template file 
in the root directory that is from a working installation.

It defaults to vi keybindings. Use ? to show a popup help window.

The fuzzy search function could be improved.
