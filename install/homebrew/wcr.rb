class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.94"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.94/wcr-macos-x86_64.tar.gz"
      sha256 "be141c900ebd73888cc2a5cf283d403376c41321fcdc9eb0dc11ce1393d993d0"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.94/wcr-macos-aarch64.tar.gz"
      sha256 "ebfa16c6b841bf0112e95a7f20c88da408e7ef2d66f52064ca5dcb2f8b462247"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.94/wcr-linux-x86_64.tar.gz"
      sha256 "21e2eb5ea16e200b562fe2713b78cab18347094813d19a4a3e9a4e6fcef7e0c2"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.94/wcr-linux-aarch64.tar.gz"
      sha256 "64865ed2332d1e2cd4fa5a207406271522383f1f034f6e173469ae5d20c48a5f"
    end
  end

  def install
    bin.install "wcr"
    bin.install "wcr-gui" if File.exist?("wcr-gui")
    bin.install "modem73" if File.exist?("modem73")
    bin.install "radio.py"
  end

  def caveats
    <<~EOS
      Run `wcr setup` once, then `wcr gui` or `wcr tui`.
      Update sha256 lines in this formula after each tagged release (see release CI).
    EOS
  end

  test do
    system "#{bin}/wcr", "--version"
  end
end
