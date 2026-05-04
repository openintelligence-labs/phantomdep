# headline.tape — the 15-second README GIF.
#
# Renders to assets/demos/headline.gif. Story:
#   1. Show a phantom + a real install command together.
#   2. PhantomDep flags the phantom + suggests the real package.
#   3. Exit code 2 — install was blocked.
#
# Render with:    vhs assets/demos/tapes/headline.tape
# Install VHS:    brew install vhs

Output assets/demos/headline.gif
Output assets/demos/headline.mp4

Set Theme "Dracula"
Set FontSize 16
Set Width 1100
Set Height 540
Set Padding 24
Set TypingSpeed 35ms
Set Shell "bash"

# Hide the prompt + alias setup so the recording opens on a clean shell.
Hide
Type "export PS1='$ '"
Enter
Type "alias phantomdep='./target/release/phantomdep'"
Enter
Type "clear"
Enter
Show

Sleep 600ms

Type "phantomdep wrap pip install requests fastapi huggingface-cli phantom-pkg-12345"
Sleep 400ms
Enter
Sleep 4500ms

Type "echo $?"
Sleep 200ms
Enter
Sleep 2000ms
