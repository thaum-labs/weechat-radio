class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.106"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.106/wcr-macos-x86_64.tar.gz"
      sha256 "7a9e96503cd96018fbf429d47487c2e6d9d0bc370e0405362e3f2a571a66dcf4"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.106/wcr-macos-aarch64.tar.gz"
      sha256 "e1bf6aace5fef66d9d5c588cb95357885f8788da96cfd7dd21e9159c90bd7976"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.106/wcr-linux-x86_64.tar.gz"
      sha256 "c22f823bd1c3c8a5c1af7bf4120ff3770c8eef8ea756bbeec1eade6978432817"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.106/wcr-linux-aarch64.tar.gz"
      sha256 "2ee8b9fa4a11c59a58240897c5c99e669120879ad0ab87bc281125d781594914"
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
