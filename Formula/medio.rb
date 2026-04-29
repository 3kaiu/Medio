class Medio < Formula
  desc "Media file manager: rename, deduplicate, organize"
  homepage "https://github.com/3kaiu/Medio"
  license "MIT"
  head "https://github.com/3kaiu/Medio.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--locked"
    bin.install "target/release/medio"
    bin.install_symlink bin/"medio" => "me"
  end

  test do
    system "#{bin}/medio", "--version"
  end
end
