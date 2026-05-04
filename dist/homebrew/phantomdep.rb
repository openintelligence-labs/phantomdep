# Homebrew formula for PhantomDep.
#
# Lives at openintelligence-labs/homebrew-tap/Formula/phantomdep.rb once the
# tap repo is created. Until then, this is the canonical template.
#
# To update: bump VERSION, regenerate the SHA256 lines from the new release
# assets (`shasum -a 256 phantomdep-<target>.tar.gz`), and open a PR against
# the tap repo.

class Phantomdep < Formula
  desc "Local-first dependency firewall for AI coding agents"
  homepage "https://github.com/openintelligence-labs/phantomdep"
  version "1.0.0" # TODO: bump on each release.
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_DARWIN_SHA256"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_X86_64_DARWIN_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_LINUX_SHA256"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_X86_64_LINUX_SHA256"
    end
  end

  def install
    bin.install "phantomdep"
  end

  test do
    # `phantomdep doctor` exits non-zero (because it intentionally surfaces
    # phantom packages), so we just confirm the binary launches cleanly.
    output = shell_output("#{bin}/phantomdep --version")
    assert_match "phantomdep #{version}", output
  end
end
