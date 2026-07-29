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
      sha256 "f8d4212b2393b69efb63b749e674ae2042722b794c2defb0122407cb99f59821"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-apple-darwin.tar.gz"
      sha256 "cfec8f19eea7d9ffa4d877c65a0e82bf184964c8102a59d6d6f87d4d98041896"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "982121c01f6b99f7f9b07071ca78e51fd80a3f553adeaa816e7f51ebfc1384f9"
    end
    on_intel do
      url "https://github.com/openintelligence-labs/phantomdep/releases/download/v#{version}/phantomdep-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "d393bc850c9310e903303f2699d9c4f8edd24b1cc0171d69451400a59d26a9d6"
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
