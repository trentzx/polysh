# Homebrew formula. The live copy is in https://github.com/trentzx/homebrew-polysh
# (Formula/polysh.rb); keep the two in sync on each release.
class Polysh < Formula
  desc "Type Linux, Windows or PowerShell commands on any OS; polysh translates them"
  homepage "https://github.com/trentzx/polysh"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/trentzx/polysh/releases/download/v0.3.0/polysh-v0.3.0-aarch64-apple-darwin.tar.gz"
      sha256 "210f0f18ae48e60e16b4b8a41aae0bc749c48f6a18c12520c95afd652d3d02e9"
    end
    on_intel do
      url "https://github.com/trentzx/polysh/releases/download/v0.3.0/polysh-v0.3.0-x86_64-apple-darwin.tar.gz"
      sha256 "012d6bda2978cd29270e8f23f1f25b7fe95bb67c0ddb685c6ef794b2c94f43ee"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/trentzx/polysh/releases/download/v0.3.0/polysh-v0.3.0-x86_64-unknown-linux-musl.tar.gz"
      sha256 "70466852018ae18346e56758f26875e349f5383de6bbeca8efd2204371da464e"
    end
  end

  def install
    bin.install "polysh"
  end

  test do
    assert_equal "dir /A", shell_output("#{bin}/polysh -n --to cmd ls -la 2>/dev/null").strip
  end
end
