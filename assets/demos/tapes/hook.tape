# hook.tape — Claude Code PreToolUse hook intercept (~12 s).
#
# Story: AI assistant tries `pip install huggingface-cli` → PhantomDep's
# hook blocks it before the install runs and tells the AI exactly which
# package failed and what the real package is.

Output assets/demos/hook.gif

Set Theme "Dracula"
Set FontSize 15
Set Width 1100
Set Height 480
Set Padding 24
Set TypingSpeed 30ms
Set Shell "bash"

Hide
Type "export PS1='$ '"
Enter
Type "alias phantomdep='./target/release/phantomdep'"
Enter
Type "cp assets/demos/fixtures/hook-event.json /tmp/event.json"
Enter
Type "clear"
Enter
Show

Sleep 600ms

Type "phantomdep hook check < /tmp/event.json"
Sleep 400ms
Enter
Sleep 3500ms

Type `echo "exit: $?"`
Sleep 200ms
Enter
Sleep 1500ms
