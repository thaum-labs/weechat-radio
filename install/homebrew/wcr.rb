class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.97"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.97/wcr-macos-x86_64.tar.gz"
      sha256 "b9a32eef968ff46ac4b8a720ffa27c407b8b58ba1931d09ecdc34ba4273dd53f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.97/wcr-macos-aarch64.tar.gz"
      sha256 "05ebaf68d7da12ee151546ba99d0deecbce5957f0e17637959a13738a6bd14a6"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.97/wcr-linux-x86_64.tar.gz"
      sha256 "ef04d10a7edb6f6bc9deb067a7dcc9eab7c2cd6c8fc54f59489265c0b5a8b680"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.97/wcr-linux-aarch64.tar.gz"
      sha256 "644d81881304851e8594d282141a79740c3cc45b156c14f77abce13b7031f097"
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
