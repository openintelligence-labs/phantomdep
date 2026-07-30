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
  version "1.0.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-aarch64-apple-darwin.tar.gz"
      sha256 "e25fb2b7f38f14fe377fe2a7158f0165eec77a2e4775c622ff691b2ddbe70102"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-apple-darwin.tar.gz"
      sha256 "367381953855b51ecb7c717ca07e53007682da617829182fedb63ecaada166c2"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "525aca4e4912587bec5301c4efd13a39f11aad9aef4ea1cadcc379704f47e8c7"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "eb058e92546ce7ec06d96d98b21bf48f545104e1512da7e38a6c6f72d49acc57"
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
