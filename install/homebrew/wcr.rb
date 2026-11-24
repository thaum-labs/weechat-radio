class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.22"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.22/wcr-macos-x86_64.tar.gz"
      sha256 "801640c9bc3fb2be7a6ff3b70ff661065b5da05f6cd079dea09894c4a3f987a2"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.22/wcr-macos-aarch64.tar.gz"
      sha256 "24d644ad0276c9ab928374a2f8315d3b6fce8b8255bc514bdaae6f171da96caa"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.22/wcr-linux-x86_64.tar.gz"
      sha256 "d88286a7b52bc62328353b62de674afd8033f5799fcfd0ae8179c670f340d4b3"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.22/wcr-linux-aarch64.tar.gz"
      sha256 "b61b6f0091811e7809ffe50519c8b4cb3fc1a0e4caa79f6e93b88ecd4bf82131"
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
