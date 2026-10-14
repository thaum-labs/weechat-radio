class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.83"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.83/wcr-macos-x86_64.tar.gz"
      sha256 "faa0b83fa83bba4741e77ce5a916ef3b452290223771b7c8ae26187c87789e4a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.83/wcr-macos-aarch64.tar.gz"
      sha256 "9657f29bd50899e2fd6e644f572cfa70d1fc5677344de9439e0ca63cb5b405d6"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.83/wcr-linux-x86_64.tar.gz"
      sha256 "da192a5805814e0419501e1119ac9134273a3a42dd05a16f164d33566b0ef279"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.83/wcr-linux-aarch64.tar.gz"
      sha256 "d8115cf807ba88d483ffa17fa8a3735e31574557f5b397d2eef8c256ea590160"
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
