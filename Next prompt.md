Continue the work on BetterWriter. Plan.md shows the current state of the project and the current goals, README says the overall goals. Analyze its structure. I added to the plan this stage, focus on it, make a:

Starting menu with recently opened projects kept in a vertical list, double-clicking opens the project to the edit menu we currently have. There should be a button to create new project with options: 1: name ("Untitled" by default, if it already exists add 1 then 2, etc.), 2. First track instrument Currently only Stringed will work, but also add Orchestra, Drums, MIDI. Stringed has options of Acoustic guitar, Electric guitar, Bass, Other. In the future each will have default playback sound that will be modified with VST3s. 3. Amount of strings selection (up to 12, but in the input menu make any value above inputable.) Add there a button for "Demo project" option will open what we currently have as open-on-startup. New projects are fully empty and only have 1 instrument and 1 bar at 4/4 set to default tempo of 120 bpm.

The model currently has no explicit “bar count”, which we need for project saving and opening, so implement that. 

Also inplement

&#x20;   - \[ ] Insert Bar (to the left of selected bar) (`Ctrl+Ins` and `Ctrl+Shift+B`)

&#x20;   - \[ ] Add a Bar (to the right of selected bar) (`Ctrl+B`)

&#x20;   - \[ ] Delete Bar (`Ctrl+Del`)

From RMB menu, because we're gonna need a way to add and remove bars now.

