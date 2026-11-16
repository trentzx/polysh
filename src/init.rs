//! `polysh init <shell>`: snippets that hook polysh into the shell you
//! already use, so a command it doesn't know gets translated instead of
//! failing with "command not found".
//!
//! Every hook calls `polysh --hook -- <words...>`, which runs the translation
//! or exits 127 when there is none. The hook then falls back to whatever the
//! shell would have done without polysh (including any handler that was
//! installed before, such as Ubuntu's package suggestions).

pub const SHELLS: &str = "bash, zsh, fish, powershell, cmd";

pub fn snippet(shell: &str) -> Option<&'static str> {
    match shell.to_ascii_lowercase().as_str() {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        "powershell" | "pwsh" | "ps" => Some(POWERSHELL),
        "cmd" => Some(CMD),
        _ => None,
    }
}

const BASH: &str = r#"# polysh: run Windows and PowerShell commands bash doesn't know.
# Add to ~/.bashrc:  eval "$(polysh init bash)"
case "$(declare -f command_not_found_handle 2>/dev/null)" in
  *"polysh --hook"*) ;;
  ?*) eval "__polysh_prev_cnf() $(declare -f command_not_found_handle | tail -n +2)" ;;
esac
command_not_found_handle() {
  polysh --hook -- "$@"
  local s=$?
  [ "$s" -ne 127 ] && return "$s"
  if declare -F __polysh_prev_cnf >/dev/null; then
    __polysh_prev_cnf "$@"
    return $?
  fi
  printf 'bash: %s: command not found\n' "$1" >&2
  return 127
}
"#;

const ZSH: &str = r#"# polysh: run Windows and PowerShell commands zsh doesn't know.
# Add to ~/.zshrc:  eval "$(polysh init zsh)"
case "${functions[command_not_found_handler]}" in
  *"polysh --hook"*) ;;
  ?*) functions[__polysh_prev_cnf]="${functions[command_not_found_handler]}" ;;
esac
command_not_found_handler() {
  polysh --hook -- "$@"
  local s=$?
  (( s != 127 )) && return $s
  if (( ${+functions[__polysh_prev_cnf]} )); then
    __polysh_prev_cnf "$@"
    return $?
  fi
  print -u2 "zsh: command not found: $1"
  return 127
}
"#;

const FISH: &str = r#"# polysh: run Windows and PowerShell commands fish doesn't know.
# Add to ~/.config/fish/config.fish:  polysh init fish | source
if not functions -q __polysh_prev_cnf
    and not string match -q '*polysh --hook*' -- (functions fish_command_not_found)
    functions -c fish_command_not_found __polysh_prev_cnf
end
function fish_command_not_found
    polysh --hook -- $argv
    set -l s $status
    if test $s -ne 127
        return $s
    end
    if functions -q __polysh_prev_cnf
        __polysh_prev_cnf $argv
    else
        echo "fish: Unknown command: $argv[1]" >&2
    end
end
"#;

// PowerShell already has aliases for ls, cp, rm, cat and friends, so this
// only kicks in for names it really doesn't know (grep, touch, head, which,
// df, ...). The translation is run inside PowerShell, so pipes keep working.
const POWERSHELL: &str = r#"# polysh: run Linux/macOS and cmd commands PowerShell doesn't know.
# Add to $PROFILE:  Invoke-Expression (& polysh init powershell | Out-String)
$ExecutionContext.InvokeCommand.CommandNotFoundAction = {
    param($Name, $EventArgs)
    # PowerShell asks about 'get-<name>' first; only answer for the real name.
    if ($Name -eq 'polysh' -or $Name -like 'get-*') { return }
    $n = $Name
    $EventArgs.CommandScriptBlock = {
        $t = & polysh --hook --print --to powershell -- $n @args
        if ($LASTEXITCODE -ne 0 -or -not $t) {
            Write-Error "The term '$n' is not recognized as a name of a cmdlet, function, script file, or executable program." -Category ObjectNotFound
            return
        }
        $code = ($t -join "`n")
        if ($MyInvocation.ExpectingInput) { $code = '$input | ' + $code }
        $input | & ([scriptblock]::Create($code))
    }.GetNewClosure()
    $EventArgs.StopSearch = $true
}
"#;

// cmd has no "command not found" hook, so these are doskey macros for the
// Linux/macOS names polysh knows. They apply to the first word of a line.
const CMD: &str = r#"ls=polysh -c ls $*
cp=polysh -c cp $*
mv=polysh -c mv $*
rm=polysh -c rm $*
cat=polysh -c cat $*
touch=polysh -c touch $*
grep=polysh -c grep $*
which=polysh -c which $*
clear=polysh -c clear $*
ps=polysh -c ps $*
kill=polysh -c kill $*
pkill=polysh -c pkill $*
killall=polysh -c killall $*
head=polysh -c head $*
tail=polysh -c tail $*
wc=polysh -c wc $*
uname=polysh -c uname $*
df=polysh -c df $*
ln=polysh -c ln $*
open=polysh -c open $*
xdg-open=polysh -c xdg-open $*
ifconfig=polysh -c ifconfig $*
sleep=polysh -c sleep $*
env=polysh -c env $*
pwd=polysh -c pwd $*
"#;
