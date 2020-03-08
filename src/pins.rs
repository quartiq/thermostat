use stm32f4xx_hal::gpio::{
    gpioa::{PA1, PA2, PA7},
    gpiob::{PB11, PB13},
    gpioc::{PC1, PC4, PC5},
    gpiog::{PG13},
    Speed::VeryHigh,
};

pub fn setup_ethernet<M1, M2, M3, M4, M5, M6, M7, M8, M9>(
    pa1: PA1<M1>, pa2: PA2<M2>, pc1: PC1<M3>, pa7: PA7<M4>,
    pc4: PC4<M5>, pc5: PC5<M6>, pb11: PB11<M7>, pg13: PG13<M8>,
    pb13: PB13<M9>
) {
    // PA1 RMII Reference Clock - SB13 ON
    pa1.into_alternate_af11().set_speed(VeryHigh);
    // PA2 RMII MDIO - SB160 ON
    pa2.into_alternate_af11().set_speed(VeryHigh);
    // PC1 RMII MDC - SB164 ON
    pc1.into_alternate_af11().set_speed(VeryHigh);
    // PA7 RMII RX Data Valid D11 JP6 ON
    pa7.into_alternate_af11().set_speed(VeryHigh);
    // PC4 RMII RXD0 - SB178 ON
    pc4.into_alternate_af11().set_speed(VeryHigh);
    // PC5 RMII RXD1 - SB181 ON
    pc5.into_alternate_af11().set_speed(VeryHigh);
    // PB11 RMII TX Enable - SB183 ON
    pb11.into_alternate_af11().set_speed(VeryHigh);
    // PG13 RXII TXD0 - SB182 ON
    pg13.into_alternate_af11().set_speed(VeryHigh);
    // PB13 RMII TXD1 I2S_A_CK JP7 ON
    pb13.into_alternate_af11().set_speed(VeryHigh);
}
