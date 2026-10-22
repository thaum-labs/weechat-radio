class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.109"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.109/wcr-macos-x86_64.tar.gz"
      sha256 "854e37dd3c3ebba76589b878110d608d86da635de4f54c6e9231d6c412475e0f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.109/wcr-macos-aarch64.tar.gz"
      sha256 "d27173845d29a5ced2fc7f285d8726f44fc514a8bba2b93949cf3e0d5d208199"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.109/wcr-linux-x86_64.tar.gz"
      sha256 "ae9ea8bb169f6c8fd125ac508bb0e16513d7481c0ad3f12de96c1c7378c75a06"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.109/wcr-linux-aarch64.tar.gz"
      sha256 "0653706cc883a239c800b70e23ef4f81b473dac48f4541765ae824cbb40fd05e"
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
