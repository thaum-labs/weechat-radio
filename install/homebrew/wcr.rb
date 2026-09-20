class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.14"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-x86_64.tar.gz"
      sha256 "cb05e59a01c79ba28c74a22abce0ea23c73427c45e2a59ca04060bbcbf51bd67"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-aarch64.tar.gz"
      sha256 "4b1f14251d43fe5e6a68db9227db8bcd900ac09cb05262996e10d46392a51fe8"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-x86_64.tar.gz"
      sha256 "c24d20ce36d7bcba7c3c46ad6ba1b6c140570bfbda9a8741592cf457ed2e654e"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-aarch64.tar.gz"
      sha256 "4f591a28643276a3b1f0d653160bed429a0fa37993d94cb8f3254d952cd93c01"
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
