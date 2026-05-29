class Plz < Formula
  desc "Natural-language to shell-command CLI"
  homepage "https://github.com/sagwaco/pretty-plz"
  url "https://github.com/sagwaco/pretty-plz.git",
      tag:      "v0.1.0",
      revision: "6b1af1123c65687b573c0ecbc59829bd6d7ed768"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/plz --version")
  end
end
