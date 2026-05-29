class Plz < Formula
  desc "Natural-language to shell-command CLI"
  homepage "https://github.com/sagwaco/pretty-plz"
  url "https://github.com/sagwaco/pretty-plz.git",
      tag:      "v0.1.1",
      revision: "a397f61f402ede5a93dbfc042158a61a3adbdecc"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/plz --version")
  end
end
