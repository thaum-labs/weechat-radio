class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.112"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.112/wcr-macos-x86_64.tar.gz"
      sha256 "49af07c7c2f5ebd0f736d6d06520341ca47298887494564ceb006565bc141ff7"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.112/wcr-macos-aarch64.tar.gz"
      sha256 "bee90c2402b876e7dbf018dd1729203e7aa7eb903fa6d09e3600a998917ce2d1"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.112/wcr-linux-x86_64.tar.gz"
      sha256 "feafe99a293dfb9c49d4b405941448c5be20adabf68cc8c5fbaabc3682fe44d5"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.112/wcr-linux-aarch64.tar.gz"
      sha256 "b619af0812b15cc1185bf6511ad368fedfe7965b1c9994aec82a4c777c136f10"
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
