# replay.tape — full Phantom-DB replay + benchmark (~14 s).
#
# Useful for "look how fast and how much it catches" social posts.

Output assets/demos/replay.gif

Set Theme "Dracula"
Set FontSize 14
Set Width 1100
Set Height 760
Set Padding 24
Set TypingSpeed 30ms
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

Type "phantomdep replay"
Sleep 300ms
Enter
Sleep 4500ms

Type "phantomdep benchmark --iterations 50"
Sleep 300ms
Enter
Sleep 6000ms
