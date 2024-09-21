# tiny-wb

Simple tinySA to Workbench converter

# First window
> Selection

Starts with the most likely com port selected (last used, then usual suspect, then port 0)
Enter selects it (in the validation sense)
Selects the first likely baudrate (same algo)
Enter selects it
Selects the validate text box

First selection should be enter, then with L/R U/D (or mouse?) you can change the params
(P) to select a port (B) to select a baudrate (Enter) to be put on the <Next> selection

# Second window
> Customisation

Tab/L/R for navigation, Enter to change a parameter
Starts with <Scan> selected and either (L/R/U/D) to be put on the fields

# Third window
> Result

Show a progress bar, then show a nice message with a prompt to ask where to save, then exits

# References

* First line is discardable as it contains only a repead of sent command
* Read needs to stop on "ch>" 99, 104, 62
* Need to send resume afterward
* Doc: https://tinysa.org/wiki/pmwiki.php?n=Main.USBInterface
* Working example: http://athome.kaashoek.com/tinySA/python/
* Firmwares: All firmware files are located in http://athome.kaashoek.com/tinySA/DFU/
* QT5 GUI:https://github.com/g4ixt/QtTinySA?tab=readme-ov-file
* TUI: Rattatui
* ProgressB: Indicatif
* Version: tinySA4: Ultra, tinySA_ Tiny
* Scan will start with 123 and end with 125
* https://groups.io/g/tinysa
* https://github.com/Ho-Ro/tinySA/blob/main/sa_cmd.c
* NANOVNA: https://github.com/ttrftech/NanoVNA?tab=readme-ov-file
