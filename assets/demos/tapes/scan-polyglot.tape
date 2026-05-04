# scan-polyglot.tape — scan a 4-ecosystem fixture (~12 s).
#
# Story: PhantomDep walks Python + TypeScript + Rust + Go in one pass and
# correctly classifies real vs phantom across all four ecosystems.

Output assets/demos/scan-polyglot.gif

Set Theme "Dracula"
Set FontSize 15
Set Width 1100
Set Height 720
Set Padding 24
Set TypingSpeed 35ms
Set Shell "bash"

Hide
Type "export PS1='$ '"
Enter
Type "alias phantomdep='./target/release/phantomdep'"
Enter
Type "clear"
Enter
Show

Sleep 600ms

Type "ls assets/demos/fixtures/"
Sleep 200ms
Enter
Sleep 1500ms

Type "phantomdep scan assets/demos/fixtures/"
Sleep 300ms
Enter
Sleep 4500ms
