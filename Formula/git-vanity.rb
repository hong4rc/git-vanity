class GitVanity < Formula
  desc "Generate Git commit hashes matching custom patterns (vanity hashes)"
  homepage "https://github.com/hong4rc/git-vanity"
  url "https://github.com/hong4rc/git-vanity/archive/refs/tags/v0.1.0.tar.gz"
  # sha256 "UPDATE_AFTER_RELEASE"
  license "LicenseRef-source-available"
  head "https://github.com/hong4rc/git-vanity.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    system "git", "init"
    system "git", "commit", "--allow-empty", "-m", "test"
    assert_match "Found matching hash", shell_output("#{bin}/git-vanity cafe --dry-run -q")
  end
end
