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
      sha256 "f554f5cd530ba91fdeba733d10af4e3198dead11f6baad22c246d0a2bb0c7842"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-apple-darwin.tar.gz"
      sha256 "ec77f7f738b0f8dfa1f068c4a20e83e73b3e6dad807f845ba0f78f0c65328ff7"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "683f0aa9713295e5033a526aca94eaaf664caee079b262d9db494b783753430a"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0c37f2d885540d3ff23cf11262ca3f0150c03fccd7f8604016ceb25c8059bd96"
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
