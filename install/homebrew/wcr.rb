class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.0"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.0/wcr-macos-x86_64.tar.gz"
      sha256 "a598dec704a8f33f401dd32cc819a08f857e8891a1cc89594089a6e830f43660"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.0/wcr-macos-aarch64.tar.gz"
      sha256 "d5e60921696ba6d1e67e61fa043eff5efcb92df21d621a744e35592fcac69fad"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.0/wcr-linux-x86_64.tar.gz"
      sha256 "9dc95df8a9d6edbf9128ee94d3763170de7a785e496e05f7f6427aeb0635f5e4"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.0/wcr-linux-aarch64.tar.gz"
      sha256 "83e71713f43982b5f954a3e07bdfd65059bb44899c4ab34ec3eba1a938b48f00"
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
