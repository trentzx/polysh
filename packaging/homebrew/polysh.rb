# Homebrew formula. The live copy is in https://github.com/trentzx/homebrew-polysh
# (Formula/polysh.rb); keep the two in sync on each release.
class Polysh < Formula
  desc "Type Linux, Windows or PowerShell commands on any OS; polysh translates them"
  homepage "https://github.com/trentzx/polysh"
  version "0.2.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/trentzx/polysh/releases/download/v0.2.0/polysh-v0.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "42b1fcbb6474632d56a854e998a7d4ca092f9f0f43ca447e153c6e0954e745a1"
    end
    on_intel do
      url "https://github.com/trentzx/polysh/releases/download/v0.2.0/polysh-v0.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "c30e4c11720038aa2b10a117d0a12a44549ccc5cd21734c8d76023405593d107"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/trentzx/polysh/releases/download/v0.2.0/polysh-v0.2.0-x86_64-unknown-linux-musl.tar.gz"
      sha256 "809c603e10baaf4149d5b80cffc8558da642dee386c2392975a2f082c77714a4"
    end
  end

  def install
    bin.install "polysh"
  end

  test do
    assert_equal "dir /A", shell_output("#{bin}/polysh -n --to cmd ls -la 2>/dev/null").strip
  end
end
