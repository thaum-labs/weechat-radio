class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  url "https://github.com/thaum-labs/weechat-radio/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/wcr")
    return unless OS.linux?

    system "curl", "-fsSL", "-o", bin/"modem73",
           "https://github.com/thaum-labs/weechat-radio/releases/latest/download/modem73-linux-x86_64"
    chmod 0755, bin/"modem73"
  end

  def caveats
    <<~EOS
      Windows and Linux installs include modem73 next to wcr.
      macOS has no official modem73 binary; get it from https://modem73.app for radio mode.
    EOS
  end

  test do
    system "#{bin}/wcr", "--version"
  end
end
