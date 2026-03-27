# copse demo
# Usage: vhs docs/demo.tape

Output docs/preview.gif

Set Width 1200
Set Height 800
Set FontSize 14
Set Theme "Builtin Dark"
Set TypingSpeed 80ms
Set Shell "zsh"

# Start copse
Type "copse"
Sleep 500ms
Enter
Sleep 2s

# Create first task
Type "n"
Sleep 500ms
Type "add-feature"
Enter
Sleep 500ms
# Select upstream
Enter
Sleep 500ms

# Create second task
Type "n"
Sleep 500ms
Type "fix-typos"
Enter
Sleep 500ms
# Select upstream
Type "j"
Sleep 500ms
Enter
Sleep 500ms

# Start tasks
Type "a"
Sleep 2s
Type "Write `hello` to foo.txt"
Enter
Sleep 5s
Enter
Sleep 5s

# Switch to diff view
Ctrl+W
Sleep 3s
Type "d"
Sleep 2s

# Add a review comment
Type "/"
Sleep 1s
Type "hello"
Sleep 500ms
Enter
Sleep 500ms
Type "o"
Sleep 1s
Type "This is a review comment"
Sleep 500ms
Ctrl+S
Sleep 1s

# Send review to agent
Type "S"
Sleep 1s
# Confirm
Type "y"
Sleep 1s

# Submit review
Enter
Sleep 2s
