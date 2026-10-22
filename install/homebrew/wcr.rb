class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.111"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.111/wcr-macos-x86_64.tar.gz"
      sha256 "860b6a52e7b2e4019b188152f20c6a6191635a0baafa68698d6a2aca9d76478c"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.111/wcr-macos-aarch64.tar.gz"
      sha256 "a4f9765c869661c992a00b63c1bfa42effed3082ac2090a3fb534cde5b414631"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.111/wcr-linux-x86_64.tar.gz"
      sha256 "853b153d11a79c4ce45d39ffdbb8ce23eb8dd61972162696f740f9d48e3e9c79"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.111/wcr-linux-aarch64.tar.gz"
      sha256 "959ca23d3585c725e960aa5d0af2550c69f9dec07a74bf9297325691822a07bb"
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
