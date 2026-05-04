# doctor.tape — show every verdict class in one shot (~15 s).
#
# Useful for "what does PhantomDep classify?" social posts.

Output assets/demos/doctor.gif

Set Theme "Dracula"
Set FontSize 14
Set Width 1100
Set Height 880
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

Type "phantomdep doctor"
Sleep 300ms
Enter
Sleep 6000ms
